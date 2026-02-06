//! TI-84 Plus CE General Purpose Timers
//!
//! Memory-mapped at 0xF20000 (port offset 0x120000 from 0xE00000)
//! Three timers share a single register block.
//!
//! Register layout (from CEmu timers.h):
//!   Timer 0: 0x00-0x0F (counter/reset/match0/match1)
//!   Timer 1: 0x10-0x1F (counter/reset/match0/match1)
//!   Timer 2: 0x20-0x2F (counter/reset/match0/match1)
//!   0x30: Control (32-bit shared)
//!   0x34: Status (32-bit, write-to-clear)
//!   0x38: Mask (32-bit)
//!   0x3C: Revision (0x00010801, read-only)
//!
//! Control bits (3 bits per timer + 3 direction bits):
//!   [0]: Timer0 enable, [1]: Timer0 clock (0=CPU, 1=32K), [2]: Timer0 overflow enable
//!   [3]: Timer1 enable, [4]: Timer1 clock, [5]: Timer1 overflow enable
//!   [6]: Timer2 enable, [7]: Timer2 clock, [8]: Timer2 overflow enable
//!   [9]: Timer0 count direction invert
//!   [10]: Timer1 count direction invert
//!   [11]: Timer2 count direction invert
//!
//! Status bits (3 bits per timer):
//!   [0]: Timer0 match0, [1]: Timer0 match1, [2]: Timer0 overflow/zero
//!   [3]: Timer1 match0, [4]: Timer1 match1, [5]: Timer1 overflow/zero
//!   [6]: Timer2 match0, [7]: Timer2 match1, [8]: Timer2 overflow/zero

/// Per-timer data registers (16 bytes each)
#[derive(Debug, Clone)]
struct TimerRegs {
    counter: u32,
    reset: u32,
    match_val: [u32; 2],
}

impl TimerRegs {
    fn new() -> Self {
        Self {
            counter: 0,
            reset: 0,
            match_val: [0; 2],
        }
    }

    fn reset(&mut self) {
        self.counter = 0;
        self.reset = 0;
        self.match_val = [0; 2];
    }
}

/// General Purpose Timer subsystem (all 3 timers)
#[derive(Debug, Clone)]
pub struct GeneralTimers {
    /// Per-timer registers
    timer: [TimerRegs; 3],
    /// Shared control register (32-bit)
    control: u32,
    /// Shared status register (32-bit, write-to-clear)
    status: u32,
    /// Interrupt mask register (32-bit)
    mask: u32,
    /// Accumulated cycles per timer (for clock division / scheduling)
    accum_cycles: [u32; 3],
    // TODO: Timer 2-cycle interrupt delay pipeline (Phase 4F)
    // CEmu uses gpt.delayStatus/gpt.delayIntrpt with SCHED_TIMER_DELAY
    // to defer status/interrupt by 2 CPU cycles. This requires scheduler
    // integration. For now, interrupts fire immediately on match/overflow.
    // Fields reserved for future implementation:
    _delay_status: u32,
    _delay_intrpt: u8,
}

/// Revision constant
const REVISION: u32 = 0x00010801;

impl GeneralTimers {
    pub fn new() -> Self {
        Self {
            timer: [TimerRegs::new(), TimerRegs::new(), TimerRegs::new()],
            control: 0,
            status: 0,
            mask: 0,
            accum_cycles: [0; 3],
            _delay_status: 0,
            _delay_intrpt: 0,
        }
    }

    pub fn reset(&mut self) {
        for t in &mut self.timer {
            t.reset();
        }
        self.control = 0;
        self.status = 0;
        self.mask = 0;
        self.accum_cycles = [0; 3];
        self._delay_status = 0;
        self._delay_intrpt = 0;
    }

    /// Check if a specific timer is enabled
    /// CEmu: control bit [i*3] is enable
    pub fn is_enabled(&self, index: usize) -> bool {
        self.control & (1 << (index * 3)) != 0
    }

    /// Check if 32kHz clock source is selected for timer
    /// CEmu: control bit [i*3+1] selects CLOCK_32K vs CLOCK_CPU
    fn uses_32k_clock(&self, index: usize) -> bool {
        self.control & (1 << (index * 3 + 1)) != 0
    }

    /// Check if count direction is inverted (down) for timer
    /// CEmu: control bit [9+index]
    fn is_inverted(&self, index: usize) -> bool {
        self.control & (1 << (9 + index)) != 0
    }

    /// Read a byte from the timer register space (0x00-0x3F)
    pub fn read(&self, addr: u32) -> u8 {
        let offset = addr & 0x3F;
        let byte = (offset & 3) as u32;
        let bit_offset = byte * 8;

        match offset {
            // Timer data registers (0x00-0x2F)
            0x00..=0x2F => {
                let timer_idx = (offset / 0x10) as usize;
                let reg = (offset % 0x10) & 0x0C;
                let value = match reg {
                    0x00 => self.timer[timer_idx].counter,
                    0x04 => self.timer[timer_idx].reset,
                    0x08 => self.timer[timer_idx].match_val[0],
                    0x0C => self.timer[timer_idx].match_val[1],
                    _ => 0,
                };
                ((value >> bit_offset) & 0xFF) as u8
            }
            // Control (32-bit)
            0x30..=0x33 => ((self.control >> bit_offset) & 0xFF) as u8,
            // Status (32-bit)
            0x34..=0x37 => ((self.status >> bit_offset) & 0xFF) as u8,
            // Mask (32-bit)
            0x38..=0x3B => ((self.mask >> bit_offset) & 0xFF) as u8,
            // Revision (32-bit)
            0x3C..=0x3F => ((REVISION >> bit_offset) & 0xFF) as u8,
            _ => 0,
        }
    }

    /// Write a byte to the timer register space (0x00-0x3F)
    pub fn write(&mut self, addr: u32, value: u8) {
        let offset = addr & 0x3F;
        let byte = (offset & 3) as u32;
        let bit_offset = byte * 8;
        let byte_mask = 0xFF_u32 << bit_offset;
        let shifted = (value as u32) << bit_offset;

        match offset {
            // Timer data registers (0x00-0x2F)
            0x00..=0x2F => {
                let timer_idx = (offset / 0x10) as usize;
                let reg = (offset % 0x10) & 0x0C;
                let target = match reg {
                    0x00 => &mut self.timer[timer_idx].counter,
                    0x04 => &mut self.timer[timer_idx].reset,
                    0x08 => &mut self.timer[timer_idx].match_val[0],
                    0x0C => &mut self.timer[timer_idx].match_val[1],
                    _ => return,
                };
                *target = (*target & !byte_mask) | (shifted & byte_mask);
            }
            // Control (32-bit)
            0x30..=0x33 => {
                self.control = (self.control & !byte_mask) | (shifted & byte_mask);
            }
            // Status (write-to-clear: writing 1s clears those bits)
            0x34..=0x37 => {
                self.status &= !(shifted & byte_mask);
            }
            // Mask (32-bit)
            0x38..=0x3B => {
                self.mask = (self.mask & !byte_mask) | (shifted & byte_mask);
            }
            // Revision is read-only
            0x3C..=0x3F => {}
            _ => {}
        }
    }

    /// Tick all timers with given CPU cycles
    /// cpu_speed: current CPU speed setting (0=6MHz, 1=12MHz, 2=24MHz, 3=48MHz)
    /// Returns a bitmask of which timer interrupts fired (bit 0=timer0, 1=timer1, 2=timer2)
    pub fn tick(&mut self, cycles: u32, cpu_speed: u8) -> u8 {
        let mut fired: u8 = 0;

        for i in 0..3 {
            if !self.is_enabled(i) {
                continue;
            }

            self.accum_cycles[i] += cycles;

            // Determine effective ticks based on clock source
            let ticks = if self.uses_32k_clock(i) {
                // 32kHz clock: convert CPU cycles to 32kHz ticks
                // cpu_cycles_per_32k_tick = cpu_rate / 32768
                let cpu_rate: u32 = match cpu_speed {
                    0 => 6_000_000,
                    1 => 12_000_000,
                    2 => 24_000_000,
                    _ => 48_000_000,
                };
                let cycles_per_tick = cpu_rate / 32_768;
                if self.accum_cycles[i] >= cycles_per_tick {
                    let t = self.accum_cycles[i] / cycles_per_tick;
                    self.accum_cycles[i] %= cycles_per_tick;
                    t
                } else {
                    continue;
                }
            } else {
                // CPU clock: 1 CPU cycle = 1 timer tick
                let t = self.accum_cycles[i];
                self.accum_cycles[i] = 0;
                t
            };

            if ticks == 0 {
                continue;
            }

            let old_counter = self.timer[i].counter;
            let inverted = self.is_inverted(i);

            if inverted {
                // Count down
                if self.timer[i].counter >= ticks {
                    self.timer[i].counter -= ticks;

                    // Check match conditions
                    self.check_matches_down(i, old_counter, self.timer[i].counter);
                } else {
                    // Underflow
                    let remaining = ticks - self.timer[i].counter;
                    self.check_matches_down(i, old_counter, 0);

                    // Check if overflow/reset enable is set (control bit i*3+2)
                    if self.control & (1 << (i * 3 + 2)) != 0 {
                        self.timer[i].counter = self.timer[i].reset.wrapping_sub(remaining);
                    } else {
                        self.timer[i].counter = 0xFFFFFFFF_u32.wrapping_sub(remaining - 1);
                    }
                    // Set overflow/zero status bit
                    self.status |= 1 << (i * 3 + 2);
                }
            } else {
                // Count up
                let (new_val, overflow) = self.timer[i].counter.overflowing_add(ticks);
                self.timer[i].counter = new_val;

                // Check match conditions
                self.check_matches_up(i, old_counter, new_val, overflow);

                if overflow {
                    // Check if overflow/reset enable is set
                    if self.control & (1 << (i * 3 + 2)) != 0 {
                        self.timer[i].counter = self.timer[i].reset.wrapping_add(new_val);
                    }
                    // Set overflow/zero status bit
                    self.status |= 1 << (i * 3 + 2);
                }
            }

            // Check if any status bits for this timer are set and masked
            let timer_status = (self.status >> (i * 3)) & 0x7;
            let timer_mask = (self.mask >> (i * 3)) & 0x7;
            if timer_status & timer_mask != 0 {
                fired |= 1 << i;
            }
        }

        fired
    }

    /// Check match conditions when counting up
    fn check_matches_up(&mut self, i: usize, old: u32, new: u32, overflow: bool) {
        for m in 0..2 {
            let match_val = self.timer[i].match_val[m];
            let crossed = if overflow {
                match_val > old || match_val <= new
            } else {
                match_val > old && match_val <= new
            };
            if crossed {
                self.status |= 1 << (i * 3 + m);
            }
        }
    }

    /// Check match conditions when counting down
    fn check_matches_down(&mut self, i: usize, old: u32, new: u32) {
        for m in 0..2 {
            let match_val = self.timer[i].match_val[m];
            if match_val >= new && match_val < old {
                self.status |= 1 << (i * 3 + m);
            }
        }
    }

    // ========== Accessors for snapshot/state persistence ==========

    pub fn counter(&self, index: usize) -> u32 {
        self.timer[index].counter
    }

    pub fn reset_value(&self, index: usize) -> u32 {
        self.timer[index].reset
    }

    pub fn match_val(&self, index: usize, m: usize) -> u32 {
        self.timer[index].match_val[m]
    }

    pub fn control_word(&self) -> u32 {
        self.control
    }

    pub fn status_word(&self) -> u32 {
        self.status
    }

    pub fn mask_word(&self) -> u32 {
        self.mask
    }

    pub fn set_counter(&mut self, index: usize, value: u32) {
        self.timer[index].counter = value;
    }

    pub fn set_reset_value(&mut self, index: usize, value: u32) {
        self.timer[index].reset = value;
    }

    pub fn set_match_val(&mut self, index: usize, m: usize, value: u32) {
        self.timer[index].match_val[m] = value;
    }

    pub fn set_control_word(&mut self, value: u32) {
        self.control = value;
    }

    pub fn set_status_word(&mut self, value: u32) {
        self.status = value;
    }

    pub fn set_mask_word(&mut self, value: u32) {
        self.mask = value;
    }

    pub fn accum_cycles(&self, index: usize) -> u32 {
        self.accum_cycles[index]
    }

    pub fn set_accum_cycles(&mut self, index: usize, value: u32) {
        self.accum_cycles[index] = value;
    }
}

impl Default for GeneralTimers {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let gpt = GeneralTimers::new();
        assert!(!gpt.is_enabled(0));
        assert!(!gpt.is_enabled(1));
        assert!(!gpt.is_enabled(2));
        assert_eq!(gpt.counter(0), 0);
    }

    #[test]
    fn test_reset() {
        let mut gpt = GeneralTimers::new();
        gpt.timer[0].counter = 0x12345678;
        gpt.control = 0xFFF;
        gpt.status = 0x1FF;
        gpt.reset();
        assert_eq!(gpt.timer[0].counter, 0);
        assert_eq!(gpt.control, 0);
        assert_eq!(gpt.status, 0);
    }

    #[test]
    fn test_read_write_counter() {
        let mut gpt = GeneralTimers::new();
        // Write counter for timer 0
        gpt.write(0x00, 0x12);
        gpt.write(0x01, 0x34);
        gpt.write(0x02, 0x56);
        gpt.write(0x03, 0x78);
        assert_eq!(gpt.timer[0].counter, 0x78563412);
        assert_eq!(gpt.read(0x00), 0x12);
        assert_eq!(gpt.read(0x03), 0x78);
    }

    #[test]
    fn test_read_write_timer1() {
        let mut gpt = GeneralTimers::new();
        // Timer 1 counter at offset 0x10
        gpt.write(0x10, 0xAB);
        gpt.write(0x11, 0xCD);
        assert_eq!(gpt.timer[1].counter & 0xFFFF, 0xCDAB);
    }

    #[test]
    fn test_control_enable() {
        let mut gpt = GeneralTimers::new();
        // Enable timer 0: bit 0 of control
        gpt.write(0x30, 0x01);
        assert!(gpt.is_enabled(0));
        assert!(!gpt.is_enabled(1));
        assert!(!gpt.is_enabled(2));

        // Enable timer 1: bit 3 of control
        gpt.write(0x30, 0x09); // bits 0 and 3
        assert!(gpt.is_enabled(0));
        assert!(gpt.is_enabled(1));
    }

    #[test]
    fn test_status_write_to_clear() {
        let mut gpt = GeneralTimers::new();
        gpt.status = 0x1FF; // All bits set
        // Write 0x07 to clear timer 0 bits
        gpt.write(0x34, 0x07);
        assert_eq!(gpt.status, 0x1F8); // Timer 0 bits cleared
    }

    #[test]
    fn test_revision() {
        let gpt = GeneralTimers::new();
        assert_eq!(gpt.read(0x3C), 0x01);
        assert_eq!(gpt.read(0x3D), 0x08);
        assert_eq!(gpt.read(0x3E), 0x01);
        assert_eq!(gpt.read(0x3F), 0x00);
    }

    #[test]
    fn test_tick_count_up() {
        let mut gpt = GeneralTimers::new();
        // Enable timer 0, count up (not inverted)
        gpt.control = 0x01; // enable timer 0
        gpt.timer[0].counter = 0;

        let fired = gpt.tick(100, 3);
        assert_eq!(fired, 0); // No interrupts
        assert_eq!(gpt.timer[0].counter, 100);
    }

    #[test]
    fn test_tick_count_down() {
        let mut gpt = GeneralTimers::new();
        // Enable timer 0, invert (count down)
        gpt.control = 0x01 | (1 << 9); // enable + direction invert
        gpt.timer[0].counter = 100;

        let fired = gpt.tick(50, 3);
        assert_eq!(fired, 0);
        assert_eq!(gpt.timer[0].counter, 50);
    }

    #[test]
    fn test_tick_overflow_sets_status() {
        let mut gpt = GeneralTimers::new();
        // Enable timer 0, count up, auto-reload
        gpt.control = 0x01 | (1 << 2); // enable + overflow enable
        gpt.timer[0].counter = 0xFFFFFFFE;
        gpt.mask = 0x04; // Mask timer 0 overflow bit

        let fired = gpt.tick(3, 3);
        // Should have overflowed
        assert_ne!(gpt.status & 0x04, 0); // Overflow bit set
        assert_ne!(fired, 0); // Interrupt fired
    }

    #[test]
    fn test_tick_underflow_with_reset() {
        let mut gpt = GeneralTimers::new();
        // Enable timer 0, count down, auto-reload
        gpt.control = 0x01 | (1 << 2) | (1 << 9); // enable + overflow + invert
        gpt.timer[0].counter = 5;
        gpt.timer[0].reset = 1000;

        gpt.tick(10, 3);
        // 5 ticks to reach 0, reload to 1000, 5 more ticks down
        assert_eq!(gpt.timer[0].counter, 995);
    }

    #[test]
    fn test_disabled_timer_no_tick() {
        let mut gpt = GeneralTimers::new();
        gpt.timer[0].counter = 100;
        // Timer 0 not enabled

        let fired = gpt.tick(50, 3);
        assert_eq!(fired, 0);
        assert_eq!(gpt.timer[0].counter, 100);
    }

    #[test]
    fn test_match_interrupt() {
        let mut gpt = GeneralTimers::new();
        // Enable timer 0, count up
        gpt.control = 0x01;
        gpt.timer[0].counter = 0;
        gpt.timer[0].match_val[0] = 50;
        gpt.mask = 0x01; // Mask match0 for timer 0

        let fired = gpt.tick(60, 3);
        assert_ne!(gpt.status & 0x01, 0); // Match0 bit set
        assert_ne!(fired, 0);
    }
}
