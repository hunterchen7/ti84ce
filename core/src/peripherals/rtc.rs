//! TI-84 Plus CE Real-Time Clock
//!
//! Memory-mapped at 0xF80000 (port offset 0x180000 from 0xE00000)
//! Also accessible via I/O port range 0x8xxx
//!
//! Based on CEmu's realclock.c implementation.
//!
//! The RTC has a 3-state machine:
//! - TICK: Processes remaining load ticks, then increments time (sec→min→hour→day)
//! - LATCH: Copies counter to latched registers, checks for pending load
//! - LOAD_LATCH: Copies load registers to latched, fires load-latch interrupt
//!
//! The RTC uses a 32.768 kHz clock. One full second is TICKS_PER_SECOND (32768) ticks.

/// Number of bits for time fields (8 bits each for sec, min, hour)
const RTC_TIME_BITS: u8 = 8 * 3; // 24 bits
/// Number of bits for all datetime fields (time + 16-bit day)
const RTC_DATETIME_BITS: u8 = RTC_TIME_BITS + 16; // 40 bits
/// Mask for all datetime bits
const RTC_DATETIME_MASK: u64 = (1u64 << RTC_DATETIME_BITS) - 1;

/// Load status gets set 1 tick after each load completes (from CEmu)
const LOAD_SEC_FINISHED: u8 = 1 + 8;      // 9 ticks for seconds
const LOAD_MIN_FINISHED: u8 = LOAD_SEC_FINISHED + 8;  // 17 ticks for minutes
const LOAD_HOUR_FINISHED: u8 = LOAD_MIN_FINISHED + 8; // 25 ticks for hours
const LOAD_DAY_FINISHED: u8 = LOAD_HOUR_FINISHED + 16; // 41 ticks for day
/// Total ticks needed to complete a full load
pub const LOAD_TOTAL_TICKS: u8 = LOAD_DAY_FINISHED + 10; // 51 ticks total
/// LOAD_PENDING = 255 (UINT8_MAX in CEmu) - indicates load just started
const LOAD_PENDING: u8 = 255;

/// 32kHz ticks per second
pub const TICKS_PER_SECOND: u64 = 32768;
/// Delay in 32kHz ticks before RTC latch event fires
pub const LATCH_TICK_OFFSET: u64 = 16429;
/// Delay for load-latch event after latch
const LOAD_LATCH_TICK_OFFSET: u64 = LATCH_TICK_OFFSET + 7;

/// RTC operating mode (matches CEmu's rtc_mode enum)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtcMode {
    /// Processing time tick and load completion
    Tick,
    /// Latch counter to latched registers
    Latch,
    /// Load-latch: copy load registers to latched
    LoadLatch,
}

/// RTC datetime representation (matches CEmu's rtc_datetime_t bit layout)
/// Stored as a packed u64: day[39:24] | hour[23:16] | min[15:8] | sec[7:0]
/// CEmu uses bitfield: day:16, hour:8, min:8, sec:8, pad:24 (little-endian order)
#[derive(Debug, Clone, Copy, Default)]
struct RtcDatetime {
    sec: u8,
    min: u8,
    hour: u8,
    day: u16,
}

impl RtcDatetime {
    /// Pack into u64 matching CEmu's bitfield layout
    /// CEmu (little-endian bitfield): day[39:24] | hour[23:16] | min[15:8] | sec[7:0]
    fn to_value(&self) -> u64 {
        (self.day as u64) << 24
            | (self.hour as u64) << 16
            | (self.min as u64) << 8
            | (self.sec as u64)
    }

    /// Unpack from u64
    fn from_value(value: u64) -> Self {
        Self {
            sec: (value & 0xFF) as u8,
            min: ((value >> 8) & 0xFF) as u8,
            hour: ((value >> 16) & 0xFF) as u8,
            day: ((value >> 24) & 0xFFFF) as u16,
        }
    }
}

/// RTC alarm (only time fields, no day)
#[derive(Debug, Clone, Copy, Default)]
struct RtcAlarm {
    sec: u8,
    min: u8,
    hour: u8,
}

impl RtcAlarm {
    /// Pack into u32 matching CEmu's rtc_time_t: hour[23:16] | min[15:8] | sec[7:0]
    fn to_value(&self) -> u32 {
        (self.hour as u32) << 16 | (self.min as u32) << 8 | (self.sec as u32)
    }
}

/// RTC Controller
#[derive(Debug, Clone)]
pub struct RtcController {
    /// Control register (bit 0 = enable, bit 6 = load, bit 7 = latch enable)
    control: u8,
    /// Interrupt status
    interrupt: u8,
    /// Load ticks processed (255 = LOAD_PENDING, >= LOAD_TOTAL_TICKS = complete)
    load_ticks_processed: u8,
    /// Current RTC mode
    mode: RtcMode,
    /// Counter (current time)
    counter: RtcDatetime,
    /// Latched values (snapshot of counter or load)
    latched: RtcDatetime,
    /// Load registers (values to load into counter)
    load: RtcDatetime,
    /// Alarm time
    alarm: RtcAlarm,
}

impl RtcController {
    /// Revision value returned at offset 0x3C-0x3F
    const REVISION: u32 = 0x00010500;

    /// Create a new RTC controller
    /// Values match CEmu's rtc_reset() which uses memset(0)
    pub fn new() -> Self {
        Self {
            control: 0,
            interrupt: 0,
            load_ticks_processed: LOAD_TOTAL_TICKS, // Load complete initially
            mode: RtcMode::Latch, // CEmu: rtc.mode = RTC_LATCH after reset
            counter: RtcDatetime::default(),
            latched: RtcDatetime::default(),
            load: RtcDatetime::default(),
            alarm: RtcAlarm::default(),
        }
    }

    /// Reset the RTC controller
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Process load ticks from startTick to endTick
    /// Matches CEmu's rtc_process_load() - bit-level transfer from load to counter
    fn process_load(&mut self, end_tick: u8) {
        let start_tick = self.load_ticks_processed;
        if end_tick <= start_tick {
            return;
        }
        self.load_ticks_processed = end_tick;
        if start_tick >= RTC_DATETIME_BITS {
            return;
        }
        let effective_end = if end_tick >= RTC_DATETIME_BITS {
            self.control &= !0x40; // Clear load bit
            RTC_DATETIME_BITS
        } else {
            end_tick
        };
        // Load is processed 1 bit at a time from most to least significant
        // writeMask = (RTC_DATETIME_MASK >> startTick) & ~(RTC_DATETIME_MASK >> endTick)
        let write_mask = (RTC_DATETIME_MASK >> start_tick) & !(RTC_DATETIME_MASK >> effective_end);
        let counter_val = self.counter.to_value();
        let load_val = self.load.to_value();
        self.counter = RtcDatetime::from_value((counter_val & !write_mask) | (load_val & write_mask));
    }

    /// Read a register byte
    /// addr is offset from controller base (0-0xFF)
    pub fn read(&mut self, addr: u32, _current_cycles: u64, _cpu_speed: u8) -> u8 {
        let index = addr & 0xFF;
        let bit_offset = ((index & 3) << 3) as u32;

        match index {
            // Latched time values
            0x00 => self.latched.sec,
            0x04 => self.latched.min,
            0x08 => self.latched.hour,
            0x0C => (self.latched.day & 0xFF) as u8,
            0x0D => ((self.latched.day >> 8) & 0xFF) as u8,

            // Alarm registers
            0x10 => self.alarm.sec,
            0x14 => self.alarm.min,
            0x18 => self.alarm.hour,

            // Control register
            0x20 => self.control,

            // Load registers
            0x24 => self.load.sec,
            0x28 => self.load.min,
            0x2C => self.load.hour,
            0x30 => (self.load.day & 0xFF) as u8,
            0x31 => ((self.load.day >> 8) & 0xFF) as u8,

            // Interrupt status
            0x34 => self.interrupt,

            // Revision (0x00010500)
            0x3C..=0x3F => ((Self::REVISION >> bit_offset) & 0xFF) as u8,

            // Load status
            0x40 => {
                let ticks = self.load_ticks_processed as i8;
                if ticks >= LOAD_TOTAL_TICKS as i8 {
                    0
                } else {
                    8 | ((ticks < LOAD_SEC_FINISHED as i8) as u8) << 4
                      | ((ticks < LOAD_MIN_FINISHED as i8) as u8) << 5
                      | ((ticks < LOAD_HOUR_FINISHED as i8) as u8) << 6
                      | ((ticks < LOAD_DAY_FINISHED as i8) as u8) << 7
                }
            }

            // Combined latched value
            0x44..=0x47 => {
                let combined = (self.latched.sec as u32)
                    | ((self.latched.min as u32) << 6)
                    | ((self.latched.hour as u32) << 12)
                    | ((self.latched.day as u32) << 17);
                ((combined >> bit_offset) & 0xFF) as u8
            }

            _ => 0,
        }
    }

    /// Write a register byte
    /// addr is offset from controller base (0-0xFF)
    pub fn write(&mut self, addr: u32, value: u8, _current_cycles: u64, _cpu_speed: u8) {
        let index = addr & 0xFF;

        match index {
            // Alarm registers (CEmu masks: sec & 63, min & 63, hour & 31)
            0x10 => self.alarm.sec = value & 63,
            0x14 => self.alarm.min = value & 63,
            0x18 => self.alarm.hour = value & 31,

            // Control register
            0x20 => {
                if value & 0x40 != 0 {
                    // Writing bit 6 starts a load operation
                    if self.control & 0x40 == 0 {
                        // Load can be pended once previous load is finished
                        if self.load_ticks_processed >= RTC_DATETIME_BITS {
                            self.load_ticks_processed = LOAD_PENDING;
                        }
                    }
                    self.control = value;
                } else {
                    // Don't allow resetting the load bit via write
                    self.control = value | (self.control & 0x40);
                }
            }

            // Load registers (CEmu masks: sec & 63, min & 63, hour & 31)
            0x24 => self.load.sec = value & 63,
            0x28 => self.load.min = value & 63,
            0x2C => self.load.hour = value & 31,
            0x30 => self.load.day = (self.load.day & 0xFF00) | (value as u16),
            0x31 => self.load.day = (self.load.day & 0x00FF) | ((value as u16) << 8),

            // Interrupt acknowledge (write to clear)
            // CEmu: intrpt_set(INT_RTC, rtc.interrupt &= ~byte)
            0x34 => {
                self.interrupt &= !value;
            }

            _ => {}
        }
    }

    /// Tick the RTC (called periodically) — unused, events are scheduler-driven
    pub fn tick(&mut self, _cycles: u32) -> bool {
        false
    }

    // === Scheduler integration methods ===

    /// Get current RTC mode
    pub fn mode(&self) -> RtcMode {
        self.mode
    }

    /// Check if a load operation was just triggered and needs scheduling
    pub fn needs_load_scheduled(&self) -> bool {
        self.load_ticks_processed == LOAD_PENDING
    }

    /// Process a scheduler event. Returns the delay (in 32kHz ticks) until the next event.
    ///
    /// Implements the CEmu rtc_event() state machine:
    /// - TICK: process remaining load, increment time if enabled, generate interrupts
    /// - LATCH: latch counter to registers, check for load, transition
    /// - LOAD_LATCH: latch load registers, fire interrupt, transition to TICK
    pub fn process_event(&mut self) -> (u64, bool) {
        let mut raise_interrupt = false;

        match self.mode {
            RtcMode::Tick => {
                // Process any remaining load operations
                if self.load_ticks_processed < LOAD_TOTAL_TICKS {
                    self.process_load(LOAD_TOTAL_TICKS);
                }

                // Next event is latch
                self.mode = RtcMode::Latch;
                let delay = LATCH_TICK_OFFSET;

                // Increment time if enabled (bit 0)
                if self.control & 1 != 0 {
                    let mut interrupts: u8 = 1; // Second interrupt always

                    self.counter.sec += 1;
                    if self.counter.sec >= 60 {
                        if self.counter.sec == 60 {
                            interrupts |= 2; // Minute rollover
                            self.counter.min += 1;
                            if self.counter.min >= 60 {
                                if self.counter.min == 60 {
                                    interrupts |= 4; // Hour rollover
                                    self.counter.hour += 1;
                                    if self.counter.hour >= 24 {
                                        if self.counter.hour == 24 {
                                            interrupts |= 8; // Day rollover
                                            self.counter.day = self.counter.day.wrapping_add(1);
                                        }
                                        self.counter.hour = 0;
                                    }
                                }
                                self.counter.min = 0;
                            }
                        }
                        self.counter.sec = 0;
                    }

                    // Check alarm match
                    // CEmu: counter.value >> (RTC_DATETIME_BITS - RTC_TIME_BITS) == alarm.value
                    let counter_time = (self.counter.to_value() >> (RTC_DATETIME_BITS - RTC_TIME_BITS)) as u32;
                    if counter_time == self.alarm.to_value() {
                        interrupts |= 16;
                    }

                    // Apply interrupt mask (control bits [5:1])
                    interrupts &= (self.control >> 1) as u8;
                    if interrupts != 0 {
                        if self.interrupt == 0 {
                            raise_interrupt = true;
                        }
                        self.interrupt |= interrupts;
                    }
                }

                (delay, raise_interrupt)
            }

            RtcMode::Latch => {
                // Latch counter to latched registers if latch enable (bit 7)
                if self.control & 128 != 0 {
                    self.latched = self.counter;
                }

                if self.control & 64 != 0 {
                    // Load pending — enable load processing
                    if self.load_ticks_processed == LOAD_PENDING {
                        self.load_ticks_processed = 0;
                    }
                    // Next event is load-latch
                    self.mode = RtcMode::LoadLatch;
                    (LOAD_LATCH_TICK_OFFSET - LATCH_TICK_OFFSET, false)
                } else {
                    // No load — next event is tick
                    self.mode = RtcMode::Tick;
                    (TICKS_PER_SECOND - LATCH_TICK_OFFSET, false)
                }
            }

            RtcMode::LoadLatch => {
                // Always latches load registers regardless of control
                self.latched = self.load;
                // Load latch complete interrupt
                self.interrupt |= 32;
                raise_interrupt = true;
                // Next event is tick
                self.mode = RtcMode::Tick;
                (TICKS_PER_SECOND - LOAD_LATCH_TICK_OFFSET, raise_interrupt)
            }
        }
    }

    /// Legacy method for compatibility - advance the load operation by one 32kHz tick
    pub fn advance_load(&mut self) {
        if self.load_ticks_processed == LOAD_PENDING {
            self.load_ticks_processed = 0;
        } else if self.load_ticks_processed < LOAD_TOTAL_TICKS {
            self.process_load(self.load_ticks_processed + 1);
        }
    }

    /// Check if more scheduler ticks are needed for the current load
    pub fn needs_more_ticks(&self) -> bool {
        self.load_ticks_processed != LOAD_PENDING
            && self.load_ticks_processed < LOAD_TOTAL_TICKS
    }

    /// Mark the load as started (called when scheduler event is first set)
    pub fn start_load_ticks(&mut self) {
        if self.load_ticks_processed == LOAD_PENDING {
            self.load_ticks_processed = 0;
        }
    }

    /// Check if interrupt is active (for intrpt_set on ack)
    pub fn has_interrupt(&self) -> bool {
        self.interrupt != 0
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

    const CPU_SPEED_48MHZ: u8 = 0x03;

    #[test]
    fn test_new() {
        let rtc = RtcController::new();
        assert_eq!(rtc.control, 0);
        assert_eq!(rtc.counter.sec, 0);
        assert_eq!(rtc.mode, RtcMode::Latch);
    }

    #[test]
    fn test_read_time() {
        let mut rtc = RtcController::new();
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
        assert_eq!(rtc.read(0x40, 0, CPU_SPEED_48MHZ), 0);
    }

    #[test]
    fn test_load_status_pending() {
        let mut rtc = RtcController::new();
        // Trigger a load
        rtc.write(0x20, 0xC1, 0, CPU_SPEED_48MHZ);
        // Load is pending (LOAD_PENDING = 255, treated as -1 in i8)
        assert_eq!(rtc.read(0x40, 0, CPU_SPEED_48MHZ), 0xF8);
    }

    #[test]
    fn test_control() {
        let mut rtc = RtcController::new();
        rtc.write(0x20, 0x01, 0, CPU_SPEED_48MHZ);
        assert_eq!(rtc.read(0x20, 0, CPU_SPEED_48MHZ), 0x01);
    }

    #[test]
    fn test_interrupt_ack() {
        let mut rtc = RtcController::new();
        rtc.interrupt = 0xFF;
        rtc.write(0x34, 0x0F, 0, CPU_SPEED_48MHZ);
        assert_eq!(rtc.interrupt, 0xF0);
    }

    #[test]
    fn test_load_bit_stays_set() {
        let mut rtc = RtcController::new();
        rtc.write(0x20, 0xC1, 0, CPU_SPEED_48MHZ);
        assert_eq!(rtc.control & 0x40, 0x40);
        // Load bit stays set until load completes
        assert_eq!(rtc.control & 0x40, 0x40);
    }

    #[test]
    fn test_alarm_write_masked() {
        let mut rtc = RtcController::new();
        rtc.write(0x10, 0xFF, 0, CPU_SPEED_48MHZ);
        assert_eq!(rtc.alarm.sec, 63);
        rtc.write(0x14, 0xFF, 0, CPU_SPEED_48MHZ);
        assert_eq!(rtc.alarm.min, 63);
        rtc.write(0x18, 0xFF, 0, CPU_SPEED_48MHZ);
        assert_eq!(rtc.alarm.hour, 31);
    }

    #[test]
    fn test_load_registers_write_read() {
        let mut rtc = RtcController::new();
        rtc.write(0x24, 30, 0, CPU_SPEED_48MHZ); // load sec
        rtc.write(0x28, 45, 0, CPU_SPEED_48MHZ); // load min
        rtc.write(0x2C, 12, 0, CPU_SPEED_48MHZ); // load hour
        rtc.write(0x30, 0x64, 0, CPU_SPEED_48MHZ); // load day low
        rtc.write(0x31, 0x00, 0, CPU_SPEED_48MHZ); // load day high
        assert_eq!(rtc.read(0x24, 0, CPU_SPEED_48MHZ), 30);
        assert_eq!(rtc.read(0x28, 0, CPU_SPEED_48MHZ), 45);
        assert_eq!(rtc.read(0x2C, 0, CPU_SPEED_48MHZ), 12);
        assert_eq!(rtc.read(0x30, 0, CPU_SPEED_48MHZ), 0x64);
        assert_eq!(rtc.read(0x31, 0, CPU_SPEED_48MHZ), 0x00);
    }

    #[test]
    fn test_time_counting_one_second() {
        let mut rtc = RtcController::new();
        // Enable RTC (bit 0) + latch enable (bit 7)
        rtc.write(0x20, 0x81, 0, CPU_SPEED_48MHZ);
        // Set counter to known time
        rtc.counter.sec = 0;
        rtc.counter.min = 0;
        rtc.counter.hour = 0;

        // Process TICK event - should increment second
        rtc.mode = RtcMode::Tick;
        let (delay, _interrupt) = rtc.process_event();
        assert_eq!(rtc.counter.sec, 1);
        assert_eq!(delay, LATCH_TICK_OFFSET);
        assert_eq!(rtc.mode, RtcMode::Latch);
    }

    #[test]
    fn test_time_counting_minute_rollover() {
        let mut rtc = RtcController::new();
        rtc.write(0x20, 0x81, 0, CPU_SPEED_48MHZ);
        rtc.counter.sec = 59;
        rtc.counter.min = 0;

        rtc.mode = RtcMode::Tick;
        rtc.process_event();

        assert_eq!(rtc.counter.sec, 0);
        assert_eq!(rtc.counter.min, 1);
    }

    #[test]
    fn test_time_counting_hour_rollover() {
        let mut rtc = RtcController::new();
        rtc.write(0x20, 0x81, 0, CPU_SPEED_48MHZ);
        rtc.counter.sec = 59;
        rtc.counter.min = 59;
        rtc.counter.hour = 0;

        rtc.mode = RtcMode::Tick;
        rtc.process_event();

        assert_eq!(rtc.counter.sec, 0);
        assert_eq!(rtc.counter.min, 0);
        assert_eq!(rtc.counter.hour, 1);
    }

    #[test]
    fn test_time_counting_day_rollover() {
        let mut rtc = RtcController::new();
        rtc.write(0x20, 0x81, 0, CPU_SPEED_48MHZ);
        rtc.counter.sec = 59;
        rtc.counter.min = 59;
        rtc.counter.hour = 23;
        rtc.counter.day = 0;

        rtc.mode = RtcMode::Tick;
        rtc.process_event();

        assert_eq!(rtc.counter.sec, 0);
        assert_eq!(rtc.counter.min, 0);
        assert_eq!(rtc.counter.hour, 0);
        assert_eq!(rtc.counter.day, 1);
    }

    #[test]
    fn test_latch_copies_counter() {
        let mut rtc = RtcController::new();
        rtc.write(0x20, 0x81, 0, CPU_SPEED_48MHZ); // Enable + latch enable
        rtc.counter.sec = 30;
        rtc.counter.min = 15;
        rtc.counter.hour = 8;
        rtc.counter.day = 100;

        rtc.mode = RtcMode::Latch;
        let (delay, _) = rtc.process_event();

        assert_eq!(rtc.latched.sec, 30);
        assert_eq!(rtc.latched.min, 15);
        assert_eq!(rtc.latched.hour, 8);
        assert_eq!(rtc.latched.day, 100);
        // No load pending -> next is TICK
        assert_eq!(rtc.mode, RtcMode::Tick);
        assert_eq!(delay, TICKS_PER_SECOND - LATCH_TICK_OFFSET);
    }

    #[test]
    fn test_load_transfer() {
        let mut rtc = RtcController::new();
        rtc.write(0x20, 0x81, 0, CPU_SPEED_48MHZ); // Enable + latch enable

        // Set load registers
        rtc.write(0x24, 30, 0, CPU_SPEED_48MHZ);
        rtc.write(0x28, 45, 0, CPU_SPEED_48MHZ);
        rtc.write(0x2C, 12, 0, CPU_SPEED_48MHZ);

        // Trigger load
        rtc.write(0x20, 0xC1, 0, CPU_SPEED_48MHZ);
        assert_eq!(rtc.load_ticks_processed, LOAD_PENDING);

        // Process LATCH event with load pending
        rtc.mode = RtcMode::Latch;
        let (delay, _) = rtc.process_event();
        assert_eq!(rtc.mode, RtcMode::LoadLatch);
        assert_eq!(delay, LOAD_LATCH_TICK_OFFSET - LATCH_TICK_OFFSET);
        assert_eq!(rtc.load_ticks_processed, 0); // Started

        // Process LOAD_LATCH event
        let (delay, raise) = rtc.process_event();
        assert!(raise); // Load-latch interrupt
        assert_eq!(rtc.interrupt & 32, 32); // Bit 5
        assert_eq!(rtc.mode, RtcMode::Tick);
        assert_eq!(delay, TICKS_PER_SECOND - LOAD_LATCH_TICK_OFFSET);

        // Process TICK event (finishes load)
        let (_delay, _) = rtc.process_event();
        // Load should be complete now, counter should have load values
        // Note: sec is 31 because TICK finishes the load (sec=30) then increments (sec=31)
        assert!(rtc.load_ticks_processed >= LOAD_TOTAL_TICKS);
        assert_eq!(rtc.counter.sec, 31);
        assert_eq!(rtc.counter.min, 45);
        assert_eq!(rtc.counter.hour, 12);
    }

    #[test]
    fn test_interrupt_types() {
        let mut rtc = RtcController::new();
        // Enable RTC + second interrupt (control bit 1)
        rtc.write(0x20, 0x83, 0, CPU_SPEED_48MHZ); // bit0=enable, bit1=sec interrupt
        rtc.counter.sec = 0;

        rtc.mode = RtcMode::Tick;
        let (_, raise) = rtc.process_event();
        assert!(raise);
        assert_eq!(rtc.interrupt & 1, 1); // Second interrupt
    }

    #[test]
    fn test_combined_latched_value() {
        let mut rtc = RtcController::new();
        rtc.latched.sec = 30;
        rtc.latched.min = 15;
        rtc.latched.hour = 8;
        rtc.latched.day = 100;

        let combined: u32 = 30 | (15 << 6) | (8 << 12) | (100 << 17);
        assert_eq!(rtc.read(0x44, 0, CPU_SPEED_48MHZ), (combined & 0xFF) as u8);
        assert_eq!(rtc.read(0x45, 0, CPU_SPEED_48MHZ), ((combined >> 8) & 0xFF) as u8);
        assert_eq!(rtc.read(0x46, 0, CPU_SPEED_48MHZ), ((combined >> 16) & 0xFF) as u8);
        assert_eq!(rtc.read(0x47, 0, CPU_SPEED_48MHZ), ((combined >> 24) & 0xFF) as u8);
    }
}
