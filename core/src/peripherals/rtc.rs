//! TI-84 Plus CE Real-Time Clock
//!
//! Memory-mapped at 0xF80000 (port offset 0x180000 from 0xE00000)
//! Also accessible via I/O port range 0x8xxx
//!
//! Based on CEmu's realclock.c implementation.
//!
//! The RTC uses a 32.768 kHz clock for timing. The RTC operates in a state machine
//! with three modes:
//! - RTC_TICK: Advances time (sec/min/hour/day) and checks alarm
//! - RTC_LATCH: Copies counter values to latched registers
//! - RTC_LOAD_LATCH: Copies load values to latched registers after a load operation
//!
//! Time advances every second (32,768 ticks at 32kHz) when enabled.
//! When a load is triggered, it takes ~51 ticks (LOAD_TOTAL_TICKS) to complete
//! loading all datetime fields.

/// Number of bits for time fields (8 bits each for sec, min, hour)
const RTC_TIME_BITS: u8 = 8 * 3; // 24 bits
/// Number of bits for all datetime fields (time + 16-bit day)
const RTC_DATETIME_BITS: u8 = RTC_TIME_BITS + 16; // 40 bits

/// Load status gets set 1 tick after each load completes (from CEmu)
/// These are the tick counts at which each field finishes loading
const LOAD_SEC_FINISHED: u8 = 1 + 8;      // 9 ticks for seconds
const LOAD_MIN_FINISHED: u8 = LOAD_SEC_FINISHED + 8;  // 17 ticks for minutes
const LOAD_HOUR_FINISHED: u8 = LOAD_MIN_FINISHED + 8; // 25 ticks for hours
const LOAD_DAY_FINISHED: u8 = LOAD_HOUR_FINISHED + 16; // 41 ticks for day
/// Total ticks needed to complete a full load
pub const LOAD_TOTAL_TICKS: u8 = LOAD_DAY_FINISHED + 10; // 51 ticks total
/// LOAD_PENDING = 255 (UINT8_MAX in CEmu) - indicates load just started
const LOAD_PENDING: u8 = 255;

/// Ticks per second at 32.768 kHz
pub const TICKS_PER_SECOND: u64 = 32768;

/// Delay in 32kHz ticks before RTC latch event fires (hardware-specific magic number from CEmu)
pub const LATCH_TICK_OFFSET: u64 = 16429;

/// Delay for load latch event after latch event
pub const LOAD_LATCH_TICK_OFFSET: u64 = LATCH_TICK_OFFSET + 7;

/// RTC state machine modes (matching CEmu's rtc_mode_t)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtcMode {
    /// Advances time counters every second
    Tick,
    /// Copies counter to latched registers
    Latch,
    /// Copies load register to latched after load operation
    LoadLatch,
}

/// RTC Controller
#[derive(Debug, Clone)]
pub struct RtcController {
    /// Control register (bit 0 = enable ticking, bit 6 = load, bit 7 = latch enable)
    control: u8,
    /// Interrupt status
    interrupt: u8,
    /// Current state machine mode
    mode: RtcMode,

    // Counter registers (current time)
    /// Counter seconds (0-59)
    counter_sec: u8,
    /// Counter minutes (0-59)
    counter_min: u8,
    /// Counter hours (0-23)
    counter_hour: u8,
    /// Counter day count (0-65535)
    counter_day: u16,

    // Latched registers (readable by software)
    /// Latched seconds (0-59)
    latched_sec: u8,
    /// Latched minutes (0-59)
    latched_min: u8,
    /// Latched hours (0-23)
    latched_hour: u8,
    /// Latched day count
    latched_day: u16,

    // Load registers (for setting time)
    /// Load seconds (0-59)
    load_sec: u8,
    /// Load minutes (0-59)
    load_min: u8,
    /// Load hours (0-23)
    load_hour: u8,
    /// Load day count
    load_day: u16,

    // Alarm registers
    /// Alarm seconds (0-59)
    alarm_sec: u8,
    /// Alarm minutes (0-59)
    alarm_min: u8,
    /// Alarm hours (0-23)
    alarm_hour: u8,

    /// Load ticks processed (255 = LOAD_PENDING, >= LOAD_TOTAL_TICKS = complete)
    load_ticks_processed: u8,
    /// CPU cycle when load was started (for timing calculation)
    #[allow(dead_code)]
    load_start_cycle: Option<u64>,
    /// Total access count for step-based timing approximation
    access_count: u64,
}

impl RtcController {
    /// Revision value returned at offset 0x3C-0x3F
    const REVISION: u32 = 0x00010500;

    /// Create a new RTC controller
    /// Values match CEmu's rtc_reset() which uses memset(0) then sets mode to RTC_LATCH
    pub fn new() -> Self {
        Self {
            control: 0, // CEmu memsets to 0
            interrupt: 0,
            mode: RtcMode::Latch, // CEmu starts in LATCH mode

            // Counter registers
            counter_sec: 0,
            counter_min: 0,
            counter_hour: 0,
            counter_day: 0,

            // Latched registers
            latched_sec: 0,
            latched_min: 0,
            latched_hour: 0,
            latched_day: 0,

            // Load registers
            load_sec: 0,
            load_min: 0,
            load_hour: 0,
            load_day: 0,

            // Alarm registers
            alarm_sec: 0,
            alarm_min: 0,
            alarm_hour: 0,

            load_ticks_processed: LOAD_TOTAL_TICKS, // Load complete initially
            load_start_cycle: None,
            access_count: 0,
        }
    }

    /// Reset the RTC controller
    pub fn reset(&mut self) {
        self.control = 0;
        self.interrupt = 0;
        self.mode = RtcMode::Latch; // CEmu starts in LATCH mode

        // Counter registers
        self.counter_sec = 0;
        self.counter_min = 0;
        self.counter_hour = 0;
        self.counter_day = 0;

        // Latched registers
        self.latched_sec = 0;
        self.latched_min = 0;
        self.latched_hour = 0;
        self.latched_day = 0;

        // Load registers
        self.load_sec = 0;
        self.load_min = 0;
        self.load_hour = 0;
        self.load_day = 0;

        // Alarm registers
        self.alarm_sec = 0;
        self.alarm_min = 0;
        self.alarm_hour = 0;

        self.load_ticks_processed = LOAD_TOTAL_TICKS;
        self.load_start_cycle = None;
        self.access_count = 0;
    }

    /// Read a register byte
    /// addr is offset from controller base (0-0xFF)
    /// current_cycles: CPU cycle count for timing calculations
    /// cpu_speed: CPU speed setting (0=6MHz, 1=12MHz, 2=24MHz, 3=48MHz)
    pub fn read(&mut self, addr: u32, current_cycles: u64, cpu_speed: u8) -> u8 {
        let index = addr & 0xFF;
        let bit_offset = ((index & 3) << 3) as u32;

        match index {
            // Latched time values
            0x00 => self.latched_sec,
            0x04 => self.latched_min,
            0x08 => self.latched_hour,
            0x0C => (self.latched_day & 0xFF) as u8,
            0x0D => ((self.latched_day >> 8) & 0xFF) as u8,

            // Alarm registers
            0x10 => self.alarm_sec,
            0x14 => self.alarm_min,
            0x18 => self.alarm_hour,

            // Control register
            0x20 => {
                self.update_load(current_cycles, cpu_speed);
                self.control
            }

            // Load registers
            0x24 => self.load_sec,
            0x28 => self.load_min,
            0x2C => self.load_hour,
            0x30 => (self.load_day & 0xFF) as u8,
            0x31 => ((self.load_day >> 8) & 0xFF) as u8,

            // Interrupt status
            0x34 => self.interrupt,

            // Revision (0x00010500)
            0x3C..=0x3F => ((Self::REVISION >> bit_offset) & 0xFF) as u8,

            // Load status
            0x40 => {
                self.update_load(current_cycles, cpu_speed);
                // Convert to i8 to treat LOAD_PENDING (255) as -1, matching CEmu
                let ticks = self.load_ticks_processed as i8;
                if ticks >= LOAD_TOTAL_TICKS as i8 {
                    0
                } else {
                    // Bits set indicate load is still in progress for each field
                    8 | ((ticks < LOAD_SEC_FINISHED as i8) as u8) << 4
                      | ((ticks < LOAD_MIN_FINISHED as i8) as u8) << 5
                      | ((ticks < LOAD_HOUR_FINISHED as i8) as u8) << 6
                      | ((ticks < LOAD_DAY_FINISHED as i8) as u8) << 7
                }
            }

            // Combined latched value
            0x44..=0x47 => {
                let combined = (self.latched_sec as u32)
                    | ((self.latched_min as u32) << 6)
                    | ((self.latched_hour as u32) << 12)
                    | ((self.latched_day as u32) << 17);
                ((combined >> bit_offset) & 0xFF) as u8
            }

            _ => 0,
        }
    }

    /// Check/update load status
    ///
    /// CEmu uses a scheduler-based system where load only progresses via
    /// scheduler events. We mirror this behavior - load only advances via
    /// advance_load() called by the scheduler, not based on CPU cycles.
    ///
    /// This function is called on reads but doesn't advance the load.
    fn update_load(&mut self, _current_cycles: u64, _cpu_speed: u8) {
        // Load only advances via scheduler events (advance_load)
        // This function is kept for API compatibility but is essentially a no-op
        // The scheduler will call advance_load() at the appropriate times
    }

    /// Write a register byte
    /// addr is offset from controller base (0-0xFF)
    /// current_cycles: CPU cycle count for timing calculations
    /// cpu_speed: CPU speed setting (0=6MHz, 1=12MHz, 2=24MHz, 3=48MHz)
    pub fn write(&mut self, addr: u32, value: u8, current_cycles: u64, cpu_speed: u8) {
        let index = addr & 0xFF;

        match index {
            // Alarm registers
            0x10 => self.alarm_sec = value & 63,
            0x14 => self.alarm_min = value & 63,
            0x18 => self.alarm_hour = value & 31,

            // Control register
            0x20 => {
                if value & 0x40 != 0 {
                    // Writing bit 6 starts a load operation
                    self.update_load(current_cycles, cpu_speed);
                    if self.control & 0x40 == 0 {
                        // Load can be pended once previous load is finished
                        // Previous load is finished when load_ticks_processed >= RTC_DATETIME_BITS
                        if self.load_ticks_processed >= RTC_DATETIME_BITS {
                            self.load_ticks_processed = LOAD_PENDING;
                            // Record when load started for timing calculation
                            self.load_start_cycle = Some(current_cycles);
                        }
                    }
                    self.control = value;
                } else {
                    // Don't allow resetting the load bit via write
                    self.control = value | (self.control & 0x40);
                }
            }

            // Load registers
            0x24 => {
                self.update_load(current_cycles, cpu_speed);
                self.load_sec = value & 63;
            }
            0x28 => {
                self.update_load(current_cycles, cpu_speed);
                self.load_min = value & 63;
            }
            0x2C => {
                self.update_load(current_cycles, cpu_speed);
                self.load_hour = value & 31;
            }
            0x30 => {
                self.update_load(current_cycles, cpu_speed);
                self.load_day = (self.load_day & 0xFF00) | (value as u16);
            }
            0x31 => {
                self.update_load(current_cycles, cpu_speed);
                self.load_day = (self.load_day & 0x00FF) | ((value as u16) << 8);
            }

            // Interrupt acknowledge (write to clear)
            0x34 => {
                self.interrupt &= !value;
            }

            _ => {}
        }
    }

    /// Tick the RTC (called periodically)
    /// For the stub, we don't actually advance time
    pub fn tick(&mut self, _cycles: u32) -> bool {
        // Return false - no interrupt pending from stub
        false
    }

    // === Scheduler integration methods ===

    /// Get the current RTC mode
    pub fn mode(&self) -> RtcMode {
        self.mode
    }

    /// Check if a scheduler event needs to be scheduled
    /// Returns true if either:
    /// - A load operation was just triggered (LOAD_PENDING)
    /// - Time ticking is enabled and RTC is in TICK mode
    pub fn needs_event_scheduled(&self) -> bool {
        // Load pending always needs scheduling
        if self.load_ticks_processed == LOAD_PENDING {
            return true;
        }
        // Time ticking needs scheduling when in LATCH mode (waiting for TICK)
        // The RTC always cycles through modes even when not ticking
        true
    }

    /// Check if a load operation was just triggered and needs scheduling
    /// Returns true if load_ticks_processed == LOAD_PENDING
    pub fn needs_load_scheduled(&self) -> bool {
        self.load_ticks_processed == LOAD_PENDING
    }

    /// Process the load operation - copy load values to counter
    /// This mimics CEmu's rtc_process_load function
    fn process_load(&mut self, end_tick: u8) {
        if end_tick > LOAD_TOTAL_TICKS {
            return;
        }
        let start_tick = self.load_ticks_processed;
        if start_tick == LOAD_PENDING {
            return; // Not started yet
        }
        if end_tick <= start_tick {
            return;
        }
        self.load_ticks_processed = end_tick;

        // Load is processed 1 bit at a time, but we simplify by copying fields
        // when their load is complete (matches CEmu's timing)
        if start_tick < LOAD_SEC_FINISHED && end_tick >= LOAD_SEC_FINISHED {
            self.counter_sec = self.load_sec;
        }
        if start_tick < LOAD_MIN_FINISHED && end_tick >= LOAD_MIN_FINISHED {
            self.counter_min = self.load_min;
        }
        if start_tick < LOAD_HOUR_FINISHED && end_tick >= LOAD_HOUR_FINISHED {
            self.counter_hour = self.load_hour;
        }
        if start_tick < LOAD_DAY_FINISHED && end_tick >= LOAD_DAY_FINISHED {
            self.counter_day = self.load_day;
        }

        // Clear load bit after all bits are loaded (at RTC_DATETIME_BITS, not LOAD_TOTAL_TICKS)
        if start_tick < RTC_DATETIME_BITS && end_tick >= RTC_DATETIME_BITS {
            self.control &= !0x40;
        }
    }

    /// Handle RTC scheduler event - returns (next_ticks, interrupt_mask)
    /// This implements CEmu's rtc_event state machine
    /// Returns the number of ticks until the next event and any interrupt bits to set
    pub fn handle_event(&mut self) -> (u64, u8) {
        let control = self.control;
        let mut interrupts: u8 = 0;

        match self.mode {
            RtcMode::Tick => {
                // Process any remaining load operations
                if self.load_ticks_processed < LOAD_TOTAL_TICKS {
                    self.process_load(LOAD_TOTAL_TICKS);
                }

                // Next event is latch
                self.mode = RtcMode::Latch;

                // Only advance time if ticking is enabled (bit 0)
                if control & 1 != 0 {
                    interrupts = 1; // Second interrupt

                    // Increment seconds
                    self.counter_sec = self.counter_sec.wrapping_add(1);
                    if self.counter_sec >= 60 {
                        if self.counter_sec == 60 {
                            interrupts |= 2; // Minute interrupt
                            self.counter_min = self.counter_min.wrapping_add(1);
                            if self.counter_min >= 60 {
                                if self.counter_min == 60 {
                                    interrupts |= 4; // Hour interrupt
                                    self.counter_hour = self.counter_hour.wrapping_add(1);
                                    if self.counter_hour >= 24 {
                                        if self.counter_hour == 24 {
                                            interrupts |= 8; // Day interrupt
                                            self.counter_day = self.counter_day.wrapping_add(1);
                                        }
                                        self.counter_hour = 0;
                                    }
                                }
                                self.counter_min = 0;
                            }
                        }
                        self.counter_sec = 0;
                    }

                    // Check alarm match (compare time fields only, not day)
                    if self.counter_sec == self.alarm_sec
                        && self.counter_min == self.alarm_min
                        && self.counter_hour == self.alarm_hour
                    {
                        interrupts |= 16; // Alarm interrupt
                    }

                    // Mask interrupts by enabled bits (control bits 1-5 enable sec/min/hour/day/alarm)
                    interrupts &= control >> 1;
                    if interrupts != 0 {
                        self.interrupt |= interrupts;
                    }
                }

                (LATCH_TICK_OFFSET, interrupts)
            }

            RtcMode::Latch => {
                // Latch counter values if latch enable is set (bit 7)
                if control & 128 != 0 {
                    self.latched_sec = self.counter_sec;
                    self.latched_min = self.counter_min;
                    self.latched_hour = self.counter_hour;
                    self.latched_day = self.counter_day;
                }

                // Check if load operation is pending (bit 6)
                if control & 64 != 0 {
                    // Enable load processing
                    if self.load_ticks_processed == LOAD_PENDING {
                        self.load_ticks_processed = 0;
                    }
                    // Next event is load latch
                    self.mode = RtcMode::LoadLatch;
                    (LOAD_LATCH_TICK_OFFSET - LATCH_TICK_OFFSET, 0)
                } else {
                    // Next event is tick
                    self.mode = RtcMode::Tick;
                    (TICKS_PER_SECOND - LATCH_TICK_OFFSET, 0)
                }
            }

            RtcMode::LoadLatch => {
                // Always latch load values regardless of control register
                self.latched_sec = self.load_sec;
                self.latched_min = self.load_min;
                self.latched_hour = self.load_hour;
                self.latched_day = self.load_day;

                // Load latch complete interrupt
                self.interrupt |= 32;

                // Next event is tick
                self.mode = RtcMode::Tick;
                (TICKS_PER_SECOND - LOAD_LATCH_TICK_OFFSET, 32)
            }
        }
    }

    /// Advance the load operation by one 32kHz tick (legacy method for compatibility)
    /// Called by the scheduler when an RTC event fires
    pub fn advance_load(&mut self) {
        if self.load_ticks_processed == LOAD_PENDING {
            // First tick after load was triggered - start at 0
            self.load_ticks_processed = 0;
        } else if self.load_ticks_processed < LOAD_TOTAL_TICKS {
            self.load_ticks_processed += 1;

            // Check if datetime bits are all loaded (matches CEmu's timing)
            // Load bit is cleared at 40 ticks (RTC_DATETIME_BITS), not 51
            if self.load_ticks_processed >= RTC_DATETIME_BITS {
                self.control &= !0x40;
            }
        }
    }

    /// Check if more scheduler ticks are needed for the current load
    pub fn needs_more_ticks(&self) -> bool {
        self.load_ticks_processed != LOAD_PENDING
            && self.load_ticks_processed < LOAD_TOTAL_TICKS
    }

    /// Mark the load as started (called when scheduler event is first set)
    /// This transitions from LOAD_PENDING to tick counting
    pub fn start_load_ticks(&mut self) {
        if self.load_ticks_processed == LOAD_PENDING {
            self.load_ticks_processed = 0;
        }
    }

    /// Get the interrupt status
    pub fn interrupt(&self) -> u8 {
        self.interrupt
    }

    // === Debug/test helpers ===

    /// Get counter seconds (for testing)
    pub fn counter_sec(&self) -> u8 {
        self.counter_sec
    }

    /// Get counter minutes (for testing)
    pub fn counter_min(&self) -> u8 {
        self.counter_min
    }

    /// Get counter hours (for testing)
    pub fn counter_hour(&self) -> u8 {
        self.counter_hour
    }

    /// Get counter days (for testing)
    pub fn counter_day(&self) -> u16 {
        self.counter_day
    }

    /// Set counter time directly (for testing)
    pub fn set_counter_time(&mut self, sec: u8, min: u8, hour: u8, day: u16) {
        self.counter_sec = sec;
        self.counter_min = min;
        self.counter_hour = hour;
        self.counter_day = day;
    }
}

impl Default for RtcController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // CPU speed constants for tests
    const CPU_SPEED_48MHZ: u8 = 0x03;

    #[test]
    fn test_new() {
        let rtc = RtcController::new();
        assert_eq!(rtc.control, 0); // CEmu memsets to 0
        assert_eq!(rtc.latched_sec, 0);
        assert_eq!(rtc.mode, RtcMode::Latch); // Starts in Latch mode
    }

    #[test]
    fn test_read_time() {
        let mut rtc = RtcController::new();
        // CEmu initializes all time values to 0
        assert_eq!(rtc.read(0x00, 0, CPU_SPEED_48MHZ), 0); // sec
        assert_eq!(rtc.read(0x04, 0, CPU_SPEED_48MHZ), 0); // min
        assert_eq!(rtc.read(0x08, 0, CPU_SPEED_48MHZ), 0); // hour
    }

    #[test]
    fn test_read_revision() {
        let mut rtc = RtcController::new();
        assert_eq!(rtc.read(0x3C, 0, CPU_SPEED_48MHZ), 0x00);
        assert_eq!(rtc.read(0x3D, 0, CPU_SPEED_48MHZ), 0x05);
        assert_eq!(rtc.read(0x3E, 0, CPU_SPEED_48MHZ), 0x01);
        assert_eq!(rtc.read(0x3F, 0, CPU_SPEED_48MHZ), 0x00);
    }

    #[test]
    fn test_load_status_complete() {
        let mut rtc = RtcController::new();
        // After new(), load should be complete
        assert_eq!(rtc.read(0x40, 0, CPU_SPEED_48MHZ), 0); // 0 means load complete
    }

    #[test]
    fn test_load_status_stays_pending() {
        let mut rtc = RtcController::new();

        // Trigger a load at cycle 0
        rtc.write(0x20, 0xC1, 0, CPU_SPEED_48MHZ); // Enable + load + latch enable

        // Immediately after trigger, load is pending (0xF8 = all fields pending + bit 3)
        assert_eq!(rtc.read(0x40, 0, CPU_SPEED_48MHZ), 0xF8);

        // Load stays pending indefinitely without scheduler events
        assert_eq!(rtc.read(0x40, 100_000, CPU_SPEED_48MHZ), 0xF8);
        assert_eq!(rtc.read(0x40, 10_000_000, CPU_SPEED_48MHZ), 0xF8);
        assert_eq!(rtc.read(0x40, 100_000_000, CPU_SPEED_48MHZ), 0xF8);
    }

    #[test]
    fn test_control() {
        let mut rtc = RtcController::new();
        rtc.write(0x20, 0x01, 0, CPU_SPEED_48MHZ); // Enable only
        assert_eq!(rtc.read(0x20, 0, CPU_SPEED_48MHZ), 0x01);
    }

    #[test]
    fn test_interrupt_ack() {
        let mut rtc = RtcController::new();
        rtc.interrupt = 0xFF;
        rtc.write(0x34, 0x0F, 0, CPU_SPEED_48MHZ); // Clear lower 4 bits
        assert_eq!(rtc.interrupt, 0xF0);
    }

    #[test]
    fn test_load_bit_stays_set() {
        let mut rtc = RtcController::new();

        // Trigger a load at cycle 0
        rtc.write(0x20, 0xC1, 0, CPU_SPEED_48MHZ);
        assert_eq!(rtc.control & 0x40, 0x40); // Load bit set

        // Load bit stays set indefinitely (load never completes without scheduler events)
        let _ = rtc.read(0x40, 10_000_000, CPU_SPEED_48MHZ);
        assert_eq!(rtc.control & 0x40, 0x40); // Load bit still set
        let _ = rtc.read(0x40, 100_000_000, CPU_SPEED_48MHZ);
        assert_eq!(rtc.control & 0x40, 0x40); // Load bit still set
    }

    // === New tests for RTC time ticking ===

    #[test]
    fn test_state_machine_initial_mode() {
        let rtc = RtcController::new();
        assert_eq!(rtc.mode, RtcMode::Latch);
    }

    #[test]
    fn test_state_machine_latch_to_tick() {
        let mut rtc = RtcController::new();
        assert_eq!(rtc.mode, RtcMode::Latch);

        // Handle latch event - should transition to Tick
        let (next_ticks, _) = rtc.handle_event();
        assert_eq!(rtc.mode, RtcMode::Tick);
        assert_eq!(next_ticks, TICKS_PER_SECOND - LATCH_TICK_OFFSET);
    }

    #[test]
    fn test_state_machine_tick_to_latch() {
        let mut rtc = RtcController::new();

        // First transition: Latch -> Tick
        rtc.handle_event();
        assert_eq!(rtc.mode, RtcMode::Tick);

        // Second transition: Tick -> Latch
        let (next_ticks, _) = rtc.handle_event();
        assert_eq!(rtc.mode, RtcMode::Latch);
        assert_eq!(next_ticks, LATCH_TICK_OFFSET);
    }

    #[test]
    fn test_time_advances_when_enabled() {
        let mut rtc = RtcController::new();

        // Enable time ticking (bit 0)
        rtc.write(0x20, 0x01, 0, CPU_SPEED_48MHZ);
        assert_eq!(rtc.counter_sec, 0);

        // Transition to Tick mode first
        rtc.handle_event(); // Latch -> Tick
        assert_eq!(rtc.mode, RtcMode::Tick);

        // Handle tick event - time should advance
        rtc.handle_event(); // Tick -> Latch, advances counter
        assert_eq!(rtc.counter_sec, 1);
        assert_eq!(rtc.mode, RtcMode::Latch);

        // Run through another full cycle
        rtc.handle_event(); // Latch -> Tick
        rtc.handle_event(); // Tick -> Latch
        assert_eq!(rtc.counter_sec, 2);
    }

    #[test]
    fn test_time_does_not_advance_when_disabled() {
        let mut rtc = RtcController::new();

        // Ticking is disabled by default (control = 0)
        assert_eq!(rtc.counter_sec, 0);

        // Transition to Tick mode
        rtc.handle_event(); // Latch -> Tick

        // Handle tick event - time should NOT advance
        rtc.handle_event(); // Tick -> Latch
        assert_eq!(rtc.counter_sec, 0); // Still 0!
    }

    #[test]
    fn test_seconds_overflow_to_minutes() {
        let mut rtc = RtcController::new();

        // Enable ticking
        rtc.write(0x20, 0x01, 0, CPU_SPEED_48MHZ);

        // Set counter to 58 seconds
        rtc.set_counter_time(58, 0, 0, 0);

        // Go to Tick mode
        rtc.handle_event(); // Latch -> Tick

        // Tick - should be 59 seconds
        rtc.handle_event(); // Tick -> Latch
        assert_eq!(rtc.counter_sec, 59);
        assert_eq!(rtc.counter_min, 0);

        // Go back to Tick mode
        rtc.handle_event(); // Latch -> Tick

        // Tick - should overflow to 0 seconds, 1 minute
        rtc.handle_event(); // Tick -> Latch
        assert_eq!(rtc.counter_sec, 0);
        assert_eq!(rtc.counter_min, 1);
    }

    #[test]
    fn test_minutes_overflow_to_hours() {
        let mut rtc = RtcController::new();

        // Enable ticking
        rtc.write(0x20, 0x01, 0, CPU_SPEED_48MHZ);

        // Set counter to 59:59
        rtc.set_counter_time(59, 59, 0, 0);

        // Go to Tick mode
        rtc.handle_event(); // Latch -> Tick

        // Tick - should overflow to 00:00:01
        rtc.handle_event(); // Tick -> Latch
        assert_eq!(rtc.counter_sec, 0);
        assert_eq!(rtc.counter_min, 0);
        assert_eq!(rtc.counter_hour, 1);
    }

    #[test]
    fn test_hours_overflow_to_days() {
        let mut rtc = RtcController::new();

        // Enable ticking
        rtc.write(0x20, 0x01, 0, CPU_SPEED_48MHZ);

        // Set counter to 23:59:59, day 0
        rtc.set_counter_time(59, 59, 23, 0);

        // Go to Tick mode
        rtc.handle_event(); // Latch -> Tick

        // Tick - should overflow to 00:00:00, day 1
        rtc.handle_event(); // Tick -> Latch
        assert_eq!(rtc.counter_sec, 0);
        assert_eq!(rtc.counter_min, 0);
        assert_eq!(rtc.counter_hour, 0);
        assert_eq!(rtc.counter_day, 1);
    }

    #[test]
    fn test_day_counter_increments() {
        let mut rtc = RtcController::new();

        // Enable ticking
        rtc.write(0x20, 0x01, 0, CPU_SPEED_48MHZ);

        // Set to end of day 100
        rtc.set_counter_time(59, 59, 23, 100);

        // Go to Tick mode
        rtc.handle_event(); // Latch -> Tick

        // Tick - day should increment to 101
        rtc.handle_event(); // Tick -> Latch
        assert_eq!(rtc.counter_day, 101);
    }

    #[test]
    fn test_latch_copies_counter_to_latched() {
        let mut rtc = RtcController::new();

        // Enable ticking and latching (bit 7)
        rtc.write(0x20, 0x81, 0, CPU_SPEED_48MHZ); // Enable + latch enable

        // Set counter to specific values
        rtc.set_counter_time(30, 45, 12, 500);

        // Latched values should be 0 initially
        assert_eq!(rtc.latched_sec, 0);

        // Handle latch event - should copy counter to latched
        rtc.handle_event(); // Latch -> Tick (this latches!)

        // Now latched values should match counter
        assert_eq!(rtc.latched_sec, 30);
        assert_eq!(rtc.latched_min, 45);
        assert_eq!(rtc.latched_hour, 12);
        assert_eq!(rtc.latched_day, 500);
    }

    #[test]
    fn test_latch_disabled_does_not_copy() {
        let mut rtc = RtcController::new();

        // Enable ticking but NOT latching
        rtc.write(0x20, 0x01, 0, CPU_SPEED_48MHZ); // Enable only

        // Set counter to specific values
        rtc.set_counter_time(30, 45, 12, 500);

        // Latched values should be 0
        assert_eq!(rtc.latched_sec, 0);

        // Handle latch event
        rtc.handle_event(); // Latch -> Tick

        // Latched values should STILL be 0 (latch disabled)
        assert_eq!(rtc.latched_sec, 0);
        assert_eq!(rtc.latched_min, 0);
        assert_eq!(rtc.latched_hour, 0);
        assert_eq!(rtc.latched_day, 0);
    }

    #[test]
    fn test_interrupt_on_second() {
        let mut rtc = RtcController::new();

        // Enable ticking + second interrupt (bit 1)
        rtc.write(0x20, 0x03, 0, CPU_SPEED_48MHZ); // Enable + second interrupt enable

        // Go to Tick mode
        rtc.handle_event(); // Latch -> Tick

        // Handle tick event - should get second interrupt
        let (_, interrupt_mask) = rtc.handle_event(); // Tick -> Latch
        assert_eq!(interrupt_mask & 1, 1); // Second interrupt
        assert_eq!(rtc.interrupt & 1, 1); // Stored in interrupt register
    }

    #[test]
    fn test_alarm_registers() {
        let mut rtc = RtcController::new();

        // Write alarm values
        rtc.write(0x10, 30, 0, CPU_SPEED_48MHZ); // alarm sec
        rtc.write(0x14, 45, 0, CPU_SPEED_48MHZ); // alarm min
        rtc.write(0x18, 12, 0, CPU_SPEED_48MHZ); // alarm hour

        // Read them back
        assert_eq!(rtc.read(0x10, 0, CPU_SPEED_48MHZ), 30);
        assert_eq!(rtc.read(0x14, 0, CPU_SPEED_48MHZ), 45);
        assert_eq!(rtc.read(0x18, 0, CPU_SPEED_48MHZ), 12);
    }

    #[test]
    fn test_load_registers() {
        let mut rtc = RtcController::new();

        // Write load values
        rtc.write(0x24, 15, 0, CPU_SPEED_48MHZ); // load sec
        rtc.write(0x28, 30, 0, CPU_SPEED_48MHZ); // load min
        rtc.write(0x2C, 8, 0, CPU_SPEED_48MHZ);  // load hour
        rtc.write(0x30, 0x12, 0, CPU_SPEED_48MHZ); // load day low byte
        rtc.write(0x31, 0x34, 0, CPU_SPEED_48MHZ); // load day high byte

        // Read them back
        assert_eq!(rtc.read(0x24, 0, CPU_SPEED_48MHZ), 15);
        assert_eq!(rtc.read(0x28, 0, CPU_SPEED_48MHZ), 30);
        assert_eq!(rtc.read(0x2C, 0, CPU_SPEED_48MHZ), 8);
        assert_eq!(rtc.read(0x30, 0, CPU_SPEED_48MHZ), 0x12);
        assert_eq!(rtc.read(0x31, 0, CPU_SPEED_48MHZ), 0x34);
    }

    #[test]
    fn test_full_day_cycle() {
        let mut rtc = RtcController::new();

        // Enable ticking
        rtc.write(0x20, 0x01, 0, CPU_SPEED_48MHZ);

        // Set to 23:59:58, day 0 - two seconds before midnight
        rtc.set_counter_time(58, 59, 23, 0);

        // Run two complete cycles (Latch -> Tick -> Latch -> Tick -> Latch)
        rtc.handle_event(); // Latch -> Tick
        rtc.handle_event(); // Tick -> Latch (now 23:59:59)
        assert_eq!(rtc.counter_sec, 59);
        assert_eq!(rtc.counter_min, 59);
        assert_eq!(rtc.counter_hour, 23);
        assert_eq!(rtc.counter_day, 0);

        rtc.handle_event(); // Latch -> Tick
        rtc.handle_event(); // Tick -> Latch (now 00:00:00 day 1)
        assert_eq!(rtc.counter_sec, 0);
        assert_eq!(rtc.counter_min, 0);
        assert_eq!(rtc.counter_hour, 0);
        assert_eq!(rtc.counter_day, 1);
    }
}
