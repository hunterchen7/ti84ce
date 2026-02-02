//! Event Scheduler for TI-84 Plus CE Emulation
//!
//! Based on CEmu's schedule.c implementation.
//! Uses a 7.68 GHz base clock rate as LCM of all hardware clocks.

/// Base clock rate: 7,680,000,000 Hz (7.68 GHz)
/// This is the LCM of all hardware clocks, allowing integer division for conversions.
pub const SCHED_BASE_CLOCK_RATE: u64 = 7_680_000_000;

/// Clock identifiers for different hardware components
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ClockId {
    /// CPU clock (variable: 6/12/24/48 MHz)
    Cpu = 0,
    /// LCD panel refresh
    Panel = 1,
    /// Run rate (for timing)
    Run = 2,
    /// 48 MHz fixed clock
    Clock48M = 3,
    /// 24 MHz fixed clock (SPI)
    Clock24M = 4,
    /// 12 MHz fixed clock
    Clock12M = 5,
    /// 6 MHz fixed clock
    Clock6M = 6,
    /// 3 MHz fixed clock
    Clock3M = 7,
    /// 1 MHz fixed clock
    Clock1M = 8,
    /// 32.768 kHz clock (RTC)
    Clock32K = 9,
}

impl ClockId {
    /// Get the clock rate in Hz for this clock ID
    /// Note: CPU clock rate depends on current speed setting
    pub fn rate(&self, cpu_speed: u8) -> u64 {
        match self {
            ClockId::Cpu => match cpu_speed {
                0 => 6_000_000,
                1 => 12_000_000,
                2 => 24_000_000,
                _ => 48_000_000,
            },
            ClockId::Panel => 60, // 60 Hz refresh (approximate)
            ClockId::Run => 1_000_000, // 1 MHz
            ClockId::Clock48M => 48_000_000,
            ClockId::Clock24M => 24_000_000,
            ClockId::Clock12M => 12_000_000,
            ClockId::Clock6M => 6_000_000,
            ClockId::Clock3M => 3_000_000,
            ClockId::Clock1M => 1_000_000,
            ClockId::Clock32K => 32_768,
        }
    }

    /// Get the number of base ticks per clock tick
    /// base_ticks = SCHED_BASE_CLOCK_RATE / clock_rate
    pub fn base_ticks_per_tick(&self, cpu_speed: u8) -> u64 {
        SCHED_BASE_CLOCK_RATE / self.rate(cpu_speed)
    }
}

/// Event identifiers for scheduled events
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum EventId {
    /// RTC load operation
    Rtc = 0,
    /// SPI transfer completion
    Spi = 1,
    /// Timer 0
    Timer0 = 2,
    /// Timer 1
    Timer1 = 3,
    /// Timer 2
    Timer2 = 4,
    /// OS Timer
    OsTimer = 5,
    /// LCD refresh
    Lcd = 6,
    /// Timer delay event (fires 2 CPU cycles after timer match)
    /// CEmu uses SCHED_TIMER_DELAY for proper interrupt ordering
    TimerDelay = 7,
    /// Number of event types
    Count = 8,
}

/// Bit 63 set indicates event is inactive
const INACTIVE_FLAG: u64 = 1 << 63;

/// A scheduled event item
#[derive(Debug, Clone)]
pub struct SchedItem {
    /// Timestamp in base ticks (bit 63 set = inactive)
    pub timestamp: u64,
    /// Clock this event uses for timing
    pub clock: ClockId,
    /// Event identifier
    pub event: EventId,
}

impl SchedItem {
    /// Create a new inactive event
    pub fn new(event: EventId, clock: ClockId) -> Self {
        Self {
            timestamp: INACTIVE_FLAG,
            clock,
            event,
        }
    }

    /// Check if this event is active (scheduled)
    pub fn is_active(&self) -> bool {
        self.timestamp & INACTIVE_FLAG == 0
    }

    /// Deactivate this event
    pub fn deactivate(&mut self) {
        self.timestamp |= INACTIVE_FLAG;
    }
}

/// The scheduler manages timed events
#[derive(Debug, Clone)]
pub struct Scheduler {
    /// All scheduled event items
    items: [SchedItem; EventId::Count as usize],
    /// Current time in base ticks
    pub base_ticks: u64,
    /// CPU cycles counter (for conversion)
    cpu_cycles: u64,
    /// Current CPU speed setting
    cpu_speed: u8,
}

impl Scheduler {
    /// Create a new scheduler
    pub fn new() -> Self {
        Self {
            items: [
                SchedItem::new(EventId::Rtc, ClockId::Clock32K),
                SchedItem::new(EventId::Spi, ClockId::Clock24M),
                SchedItem::new(EventId::Timer0, ClockId::Cpu),
                SchedItem::new(EventId::Timer1, ClockId::Cpu),
                SchedItem::new(EventId::Timer2, ClockId::Cpu),
                SchedItem::new(EventId::OsTimer, ClockId::Clock32K),
                SchedItem::new(EventId::Lcd, ClockId::Panel),
                SchedItem::new(EventId::TimerDelay, ClockId::Cpu),
            ],
            base_ticks: 0,
            cpu_cycles: 0,
            cpu_speed: 0, // Default 6 MHz
        }
    }

    /// Reset the scheduler
    pub fn reset(&mut self) {
        for item in &mut self.items {
            item.timestamp = INACTIVE_FLAG;
        }
        self.base_ticks = 0;
        self.cpu_cycles = 0;
        self.cpu_speed = 0;
    }

    /// Update CPU speed setting
    pub fn set_cpu_speed(&mut self, speed: u8) {
        self.cpu_speed = speed;
    }

    /// Get current CPU speed
    pub fn cpu_speed(&self) -> u8 {
        self.cpu_speed
    }

    /// Convert CPU cycles to base ticks
    fn cpu_cycles_to_base_ticks(&self, cycles: u64) -> u64 {
        // base_ticks = cycles * (SCHED_BASE_CLOCK_RATE / cpu_rate)
        cycles * ClockId::Cpu.base_ticks_per_tick(self.cpu_speed)
    }

    /// Convert base ticks to CPU cycles
    #[allow(dead_code)]
    fn base_ticks_to_cpu_cycles(&self, ticks: u64) -> u64 {
        // cycles = ticks / (SCHED_BASE_CLOCK_RATE / cpu_rate)
        ticks / ClockId::Cpu.base_ticks_per_tick(self.cpu_speed)
    }

    /// Advance time based on CPU cycles executed
    pub fn advance(&mut self, cpu_cycles: u64) {
        let delta_cycles = cpu_cycles.saturating_sub(self.cpu_cycles);
        self.cpu_cycles = cpu_cycles;
        self.base_ticks += self.cpu_cycles_to_base_ticks(delta_cycles);
    }

    /// Schedule an event to fire after `ticks` clock ticks
    pub fn set(&mut self, event: EventId, ticks: u64) {
        let item = &mut self.items[event as usize];
        let base_ticks_per_tick = item.clock.base_ticks_per_tick(self.cpu_speed);
        item.timestamp = self.base_ticks + ticks * base_ticks_per_tick;
    }

    /// Repeat an event (reschedule after current timestamp)
    pub fn repeat(&mut self, event: EventId, ticks: u64) {
        let item = &mut self.items[event as usize];
        let base_ticks_per_tick = item.clock.base_ticks_per_tick(self.cpu_speed);
        // Schedule relative to current timestamp, not current time
        let current = item.timestamp & !INACTIVE_FLAG;
        item.timestamp = current + ticks * base_ticks_per_tick;
    }

    /// Clear/deactivate an event
    pub fn clear(&mut self, event: EventId) {
        self.items[event as usize].deactivate();
    }

    /// Check if an event is active
    pub fn is_active(&self, event: EventId) -> bool {
        self.items[event as usize].is_active()
    }

    /// Get ticks remaining until event fires (in the event's clock domain)
    /// Returns 0 if event is not active or has already passed
    pub fn ticks_remaining(&self, event: EventId) -> u64 {
        let item = &self.items[event as usize];
        if !item.is_active() {
            return 0;
        }
        let timestamp = item.timestamp & !INACTIVE_FLAG;
        if timestamp <= self.base_ticks {
            return 0;
        }
        let base_ticks_remaining = timestamp - self.base_ticks;
        let base_ticks_per_tick = item.clock.base_ticks_per_tick(self.cpu_speed);
        base_ticks_remaining / base_ticks_per_tick
    }

    /// Check if an event has fired (timestamp reached)
    pub fn has_fired(&self, event: EventId) -> bool {
        let item = &self.items[event as usize];
        item.is_active() && (item.timestamp & !INACTIVE_FLAG) <= self.base_ticks
    }

    /// Get the next event that needs processing (if any)
    /// Returns the event ID of the earliest pending event
    pub fn next_pending_event(&self) -> Option<EventId> {
        let mut earliest: Option<(EventId, u64)> = None;

        for (idx, item) in self.items.iter().enumerate() {
            if item.is_active() {
                let timestamp = item.timestamp & !INACTIVE_FLAG;
                if timestamp <= self.base_ticks {
                    match earliest {
                        None => earliest = Some((unsafe { std::mem::transmute(idx as u8) }, timestamp)),
                        Some((_, t)) if timestamp < t => {
                            earliest = Some((unsafe { std::mem::transmute(idx as u8) }, timestamp))
                        }
                        _ => {}
                    }
                }
            }
        }

        earliest.map(|(event, _)| event)
    }

    /// Get all pending events in order
    pub fn pending_events(&self) -> Vec<EventId> {
        let mut events: Vec<(EventId, u64)> = Vec::new();

        for (idx, item) in self.items.iter().enumerate() {
            if item.is_active() {
                let timestamp = item.timestamp & !INACTIVE_FLAG;
                if timestamp <= self.base_ticks {
                    events.push((unsafe { std::mem::transmute(idx as u8) }, timestamp));
                }
            }
        }

        // Sort by timestamp (earliest first)
        events.sort_by_key(|(_, t)| *t);
        events.into_iter().map(|(e, _)| e).collect()
    }
}

// ========== State Persistence ==========

impl Scheduler {
    /// Size of scheduler state snapshot in bytes
    /// 8 (base_ticks) + 8 (cpu_cycles) + 1 (cpu_speed) + 8*8 (item timestamps) = 81 bytes, round to 88
    pub const SNAPSHOT_SIZE: usize = 88;

    /// Save scheduler state to bytes
    pub fn to_bytes(&self) -> [u8; Self::SNAPSHOT_SIZE] {
        let mut buf = [0u8; Self::SNAPSHOT_SIZE];
        let mut pos = 0;

        // Base timing state
        buf[pos..pos+8].copy_from_slice(&self.base_ticks.to_le_bytes()); pos += 8;
        buf[pos..pos+8].copy_from_slice(&self.cpu_cycles.to_le_bytes()); pos += 8;
        buf[pos] = self.cpu_speed; pos += 1;

        // Event timestamps (8 events × 8 bytes each)
        for item in &self.items {
            buf[pos..pos+8].copy_from_slice(&item.timestamp.to_le_bytes());
            pos += 8;
        }

        buf
    }

    /// Load scheduler state from bytes
    pub fn from_bytes(&mut self, buf: &[u8]) -> Result<(), i32> {
        if buf.len() < Self::SNAPSHOT_SIZE {
            return Err(-105);
        }

        let mut pos = 0;

        self.base_ticks = u64::from_le_bytes(buf[pos..pos+8].try_into().unwrap()); pos += 8;
        self.cpu_cycles = u64::from_le_bytes(buf[pos..pos+8].try_into().unwrap()); pos += 8;
        self.cpu_speed = buf[pos]; pos += 1;

        // Event timestamps
        for item in &mut self.items {
            item.timestamp = u64::from_le_bytes(buf[pos..pos+8].try_into().unwrap());
            pos += 8;
        }

        Ok(())
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clock_rates() {
        // Test fixed clocks
        assert_eq!(ClockId::Clock48M.rate(0), 48_000_000);
        assert_eq!(ClockId::Clock24M.rate(0), 24_000_000);
        assert_eq!(ClockId::Clock32K.rate(0), 32_768);

        // Test CPU clock at different speeds
        assert_eq!(ClockId::Cpu.rate(0), 6_000_000);
        assert_eq!(ClockId::Cpu.rate(1), 12_000_000);
        assert_eq!(ClockId::Cpu.rate(2), 24_000_000);
        assert_eq!(ClockId::Cpu.rate(3), 48_000_000);
    }

    #[test]
    fn test_base_ticks_per_tick() {
        // 7.68 GHz / 48 MHz = 160 base ticks per CPU tick at 48 MHz
        assert_eq!(ClockId::Clock48M.base_ticks_per_tick(0), 160);

        // 7.68 GHz / 32.768 kHz = 234,375 base ticks per RTC tick
        assert_eq!(ClockId::Clock32K.base_ticks_per_tick(0), 234_375);

        // 7.68 GHz / 24 MHz = 320 base ticks per SPI tick
        assert_eq!(ClockId::Clock24M.base_ticks_per_tick(0), 320);
    }

    #[test]
    fn test_new_scheduler() {
        let sched = Scheduler::new();
        assert_eq!(sched.base_ticks, 0);
        assert!(!sched.is_active(EventId::Rtc));
        assert!(!sched.is_active(EventId::Spi));
    }

    #[test]
    fn test_schedule_event() {
        let mut sched = Scheduler::new();
        sched.set_cpu_speed(3); // 48 MHz

        // Schedule RTC event for 10 RTC ticks (10 * 234,375 = 2,343,750 base ticks)
        sched.set(EventId::Rtc, 10);
        assert!(sched.is_active(EventId::Rtc));
        assert_eq!(sched.ticks_remaining(EventId::Rtc), 10);
        assert!(!sched.has_fired(EventId::Rtc));
    }

    #[test]
    fn test_advance_and_fire() {
        let mut sched = Scheduler::new();
        sched.set_cpu_speed(3); // 48 MHz

        // Schedule RTC event for 1 RTC tick
        sched.set(EventId::Rtc, 1);
        assert!(!sched.has_fired(EventId::Rtc));

        // Advance by enough CPU cycles to fire the event
        // 1 RTC tick = 234,375 base ticks
        // At 48 MHz, 1 CPU cycle = 160 base ticks
        // So we need 234,375 / 160 = 1,464.84 CPU cycles
        // Use 1465 to ensure we pass the threshold
        sched.advance(1465);
        assert!(sched.has_fired(EventId::Rtc));
    }

    #[test]
    fn test_clear_event() {
        let mut sched = Scheduler::new();
        sched.set(EventId::Rtc, 10);
        assert!(sched.is_active(EventId::Rtc));

        sched.clear(EventId::Rtc);
        assert!(!sched.is_active(EventId::Rtc));
    }

    #[test]
    fn test_repeat_event() {
        let mut sched = Scheduler::new();
        sched.set_cpu_speed(3); // 48 MHz

        // Schedule RTC event for 1 tick
        sched.set(EventId::Rtc, 1);
        let initial_timestamp = sched.items[EventId::Rtc as usize].timestamp;

        // Advance past the event
        sched.advance(2000);
        assert!(sched.has_fired(EventId::Rtc));

        // Repeat the event for another 1 tick
        sched.repeat(EventId::Rtc, 1);

        // New timestamp should be initial + 1 RTC tick in base ticks
        let expected = initial_timestamp + 234_375;
        assert_eq!(sched.items[EventId::Rtc as usize].timestamp, expected);
    }

    #[test]
    fn test_pending_events() {
        let mut sched = Scheduler::new();
        sched.set_cpu_speed(3); // 48 MHz

        // Schedule multiple events
        // RTC: 1 tick at 32kHz = 234,375 base ticks
        // SPI: 1000 ticks at 24MHz = 320,000 base ticks
        sched.set(EventId::Rtc, 1);
        sched.set(EventId::Spi, 1000);

        // Advance to fire RTC but not SPI
        // 1500 CPU cycles at 48MHz = 1500 * 160 = 240,000 base ticks
        // This fires RTC (234,375) but not SPI (320,000)
        sched.advance(1500);

        let pending = sched.pending_events();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0], EventId::Rtc);
    }
}
