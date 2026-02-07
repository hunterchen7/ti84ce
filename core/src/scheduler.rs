//! Event Scheduler for TI-84 Plus CE Emulation
//!
//! Based on CEmu's schedule.c implementation.
//! Uses a 7.68 GHz base clock rate as LCM of all hardware clocks.

/// Base clock rate: 7,680,000,000 Hz (7.68 GHz)
/// This is the LCM of all hardware clocks, allowing integer division for conversions.
pub const SCHED_BASE_CLOCK_RATE: u64 = 7_680_000_000;

/// Number of 32kHz ticks per second
pub const TICKS_PER_SECOND: u64 = 32_768;

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
            ClockId::Panel => 10_000_000, // 10 MHz (CEmu CLOCK_PANEL)
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
    /// Timer 2-cycle interrupt delay pipeline (CEmu: SCHED_TIMER_DELAY)
    TimerDelay = 2,
    /// Timer 0
    Timer0 = 3,
    /// Timer 1
    Timer1 = 4,
    /// Timer 2
    Timer2 = 5,
    /// OS Timer
    OsTimer = 6,
    /// LCD refresh
    Lcd = 7,
    /// LCD DMA (VRAM read)
    LcdDma = 8,
    /// Number of event types
    Count = 9,
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
    /// Current CPU speed setting
    cpu_speed: u8,
    /// Cached base ticks per CPU cycle (avoids expensive division on every advance)
    cached_cpu_base_ticks: u64,
    /// Cached earliest event timestamp (avoids scanning all events on every call)
    next_event_ticks: u64,
}

impl Scheduler {
    /// Create a new scheduler
    pub fn new() -> Self {
        Self {
            items: [
                SchedItem::new(EventId::Rtc, ClockId::Clock32K),
                SchedItem::new(EventId::Spi, ClockId::Clock24M),
                SchedItem::new(EventId::TimerDelay, ClockId::Cpu),
                SchedItem::new(EventId::Timer0, ClockId::Cpu),
                SchedItem::new(EventId::Timer1, ClockId::Cpu),
                SchedItem::new(EventId::Timer2, ClockId::Cpu),
                SchedItem::new(EventId::OsTimer, ClockId::Clock32K),
                SchedItem::new(EventId::Lcd, ClockId::Clock24M),
                SchedItem::new(EventId::LcdDma, ClockId::Clock48M),
            ],
            base_ticks: 0,
            cpu_speed: 0, // Default 6 MHz
            cached_cpu_base_ticks: ClockId::Cpu.base_ticks_per_tick(0),
            next_event_ticks: u64::MAX,
        }
    }

    /// Access scheduled items for debugging
    pub fn items(&self) -> &[SchedItem; EventId::Count as usize] {
        &self.items
    }

    /// Reset the scheduler
    pub fn reset(&mut self) {
        for item in &mut self.items {
            item.timestamp = INACTIVE_FLAG;
        }
        self.base_ticks = 0;
        self.cpu_speed = 0;
        self.cached_cpu_base_ticks = ClockId::Cpu.base_ticks_per_tick(0);
        self.next_event_ticks = u64::MAX;
    }

    /// Update CPU speed setting
    pub fn set_cpu_speed(&mut self, speed: u8) {
        self.cpu_speed = speed;
        self.cached_cpu_base_ticks = ClockId::Cpu.base_ticks_per_tick(speed);
    }

    /// Get current CPU speed
    pub fn cpu_speed(&self) -> u8 {
        self.cpu_speed
    }

    /// Recalculate the earliest event timestamp cache
    fn recalc_next_event(&mut self) {
        self.next_event_ticks = u64::MAX;
        for item in &self.items {
            if item.is_active() {
                let ts = item.timestamp & !INACTIVE_FLAG;
                if ts < self.next_event_ticks {
                    self.next_event_ticks = ts;
                }
            }
        }
    }

    /// Check if any events are pending without scanning (fast path)
    #[inline(always)]
    pub fn has_pending_events(&self) -> bool {
        self.next_event_ticks <= self.base_ticks
    }

    /// Convert all CPU-clocked event timestamps when CPU speed changes.
    ///
    /// CEmu's sched_set_clock() recalculates timestamps for all events on the changed clock.
    pub fn convert_cpu_events(&mut self, new_rate_mhz: u32, old_rate_mhz: u32) {
        if old_rate_mhz == 0 || new_rate_mhz == old_rate_mhz {
            return;
        }

        // Convert all active events on CLOCK_CPU to new timestamps
        let old_ticks_per = SCHED_BASE_CLOCK_RATE / (old_rate_mhz as u64 * 1_000_000);
        let new_ticks_per = SCHED_BASE_CLOCK_RATE / (new_rate_mhz as u64 * 1_000_000);

        for item in &mut self.items {
            if item.is_active() && item.clock == ClockId::Cpu {
                let timestamp = item.timestamp & !INACTIVE_FLAG;
                if timestamp > self.base_ticks {
                    let remaining_base = timestamp - self.base_ticks;
                    let remaining_old_ticks = remaining_base / old_ticks_per;
                    item.timestamp = self.base_ticks + remaining_old_ticks * new_ticks_per;
                }
            }
        }
        self.recalc_next_event();
    }

    /// Advance time by the given number of CPU cycles (delta, not absolute)
    #[inline(always)]
    pub fn advance(&mut self, delta_cpu_cycles: u64) {
        self.base_ticks += delta_cpu_cycles * self.cached_cpu_base_ticks;

        // SCHED_SECOND: prevent overflow by subtracting one second's worth of
        // base ticks from all timestamps every second (matches CEmu schedule.c:393-410)
        if self.base_ticks >= SCHED_BASE_CLOCK_RATE {
            self.process_second();
        }
    }

    /// Process the SCHED_SECOND event: subtract one second from all timestamps
    /// to prevent u64 overflow. CEmu does this via a dedicated scheduler event.
    fn process_second(&mut self) {
        self.base_ticks -= SCHED_BASE_CLOCK_RATE;

        // Subtract from all active event timestamps
        for item in &mut self.items {
            if item.is_active() {
                item.timestamp = item.timestamp.saturating_sub(SCHED_BASE_CLOCK_RATE);
            }
        }

        self.recalc_next_event();
    }

    /// Schedule an event to fire after `ticks` clock ticks
    pub fn set(&mut self, event: EventId, ticks: u64) {
        let item = &mut self.items[event as usize];
        let base_ticks_per_tick = item.clock.base_ticks_per_tick(self.cpu_speed);
        let ts = self.base_ticks + ticks * base_ticks_per_tick;
        item.timestamp = ts;
        if ts < self.next_event_ticks {
            self.next_event_ticks = ts;
        }
    }

    /// Repeat an event (reschedule after current timestamp)
    pub fn repeat(&mut self, event: EventId, ticks: u64) {
        let item = &mut self.items[event as usize];
        let base_ticks_per_tick = item.clock.base_ticks_per_tick(self.cpu_speed);
        // Schedule relative to current timestamp, not current time
        let current = item.timestamp & !INACTIVE_FLAG;
        let ts = current + ticks * base_ticks_per_tick;
        item.timestamp = ts;
        if ts < self.next_event_ticks {
            self.next_event_ticks = ts;
        }
    }

    /// Schedule an event relative to another event's timestamp, using the reference
    /// event's clock domain. Matches CEmu's `sched_repeat_relative(event, ref, offset, ticks)`.
    /// `offset` is in the reference event's clock ticks added to ref's timestamp,
    /// then `ticks` in the target event's own clock ticks are added.
    pub fn repeat_relative(&mut self, event: EventId, reference: EventId, offset: u64, ticks: u64) {
        let ref_ts = self.items[reference as usize].timestamp & !INACTIVE_FLAG;
        let ref_clock = self.items[reference as usize].clock;
        let ref_base_ticks = ref_clock.base_ticks_per_tick(self.cpu_speed);
        let event_base_ticks = self.items[event as usize].clock.base_ticks_per_tick(self.cpu_speed);
        let ts = ref_ts + offset * ref_base_ticks + ticks * event_base_ticks;
        self.items[event as usize].timestamp = ts;
        if ts < self.next_event_ticks {
            self.next_event_ticks = ts;
        }
    }

    /// Clear/deactivate an event
    pub fn clear(&mut self, event: EventId) {
        self.items[event as usize].deactivate();
        self.recalc_next_event();
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
    pub fn next_pending_event(&mut self) -> Option<EventId> {
        // Fast path: no events can be ready if earliest is in the future
        if self.next_event_ticks > self.base_ticks {
            return None;
        }

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

        // If we found an event, recalculate next_event_ticks after it's handled
        // (caller will clear/repeat the event, which updates the cache)
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

    /// Get the number of CPU cycles until the nearest active event fires.
    /// Returns 0 if any event has already fired or no events are active.
    /// Used by the emu loop to fast-forward HALT to the next event (matching CEmu's cpu_halt).
    pub fn cycles_until_next_event(&self) -> u64 {
        let mut earliest_timestamp: Option<u64> = None;

        for item in &self.items {
            if item.is_active() {
                let timestamp = item.timestamp & !INACTIVE_FLAG;
                match earliest_timestamp {
                    None => earliest_timestamp = Some(timestamp),
                    Some(t) if timestamp < t => earliest_timestamp = Some(timestamp),
                    _ => {}
                }
            }
        }

        match earliest_timestamp {
            Some(ts) if ts > self.base_ticks => {
                let base_ticks_remaining = ts - self.base_ticks;
                // Ceiling division to ensure we reach the event
                (base_ticks_remaining + self.cached_cpu_base_ticks - 1) / self.cached_cpu_base_ticks
            }
            _ => 0, // Event already fired or no active events
        }
    }

    /// Calculate ticks until the next RTC LATCH point in the 1-second cycle.
    ///
    /// CEmu's RTC runs on a 1-second cycle from boot, with LATCH events
    /// firing at a fixed point (LATCH_TICK_OFFSET) in each second.
    /// This function calculates how many 32kHz ticks until the next LATCH.
    ///
    /// latch_offset: The LATCH_TICK_OFFSET constant (16429)
    pub fn ticks_until_next_latch(&self, latch_offset: u64) -> u64 {
        // Get current position in 32kHz ticks
        let base_ticks_per_32k = ClockId::Clock32K.base_ticks_per_tick(self.cpu_speed); // 234,375
        let current_32k_tick = self.base_ticks / base_ticks_per_32k;

        // Position within the current 1-second cycle
        let position_in_second = current_32k_tick % TICKS_PER_SECOND;

        // Calculate ticks until next LATCH point
        if position_in_second < latch_offset {
            // LATCH hasn't happened yet this second
            latch_offset - position_in_second
        } else {
            // LATCH already happened, wait for next second
            TICKS_PER_SECOND - position_in_second + latch_offset
        }
    }
}

// ========== State Persistence ==========

impl Scheduler {
    /// Size of scheduler state snapshot in bytes
    /// 8 (base_ticks) + 1 (cpu_speed) + 9*8 (item timestamps) = 81 bytes, round to 88
    pub const SNAPSHOT_SIZE: usize = 88;

    /// Save scheduler state to bytes
    pub fn to_bytes(&self) -> [u8; Self::SNAPSHOT_SIZE] {
        let mut buf = [0u8; Self::SNAPSHOT_SIZE];
        let mut pos = 0;

        // Base timing state
        buf[pos..pos+8].copy_from_slice(&self.base_ticks.to_le_bytes()); pos += 8;
        buf[pos] = self.cpu_speed; pos += 1;

        // Event timestamps (7 events × 8 bytes each)
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
        self.cpu_speed = buf[pos]; pos += 1;
        self.cached_cpu_base_ticks = ClockId::Cpu.base_ticks_per_tick(self.cpu_speed);

        // Event timestamps
        for item in &mut self.items {
            item.timestamp = u64::from_le_bytes(buf[pos..pos+8].try_into().unwrap());
            pos += 8;
        }

        self.recalc_next_event();
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
