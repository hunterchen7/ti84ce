//! TI-84 Plus CE General Purpose Timers
//!
//! Memory-mapped at 0xF20000 (port offset 0x120000 from 0xE00000)
//! Three timers are available, each with 0x10 bytes of registers.
//!
//! Timer 1: 0xF20000-0xF2000F (counter, reset, match1, match2)
//! Timer 2: 0xF20010-0xF2001F
//! Timer 3: 0xF20020-0xF2002F
//! Global:  0xF20030-0xF2003F (control, status, mask, revision)
//!
//! Based on CEmu's core/timers.c implementation.

/// Register offsets within each timer (relative to timer base)
mod regs {
    /// Counter value (32-bit, read/write)
    pub const COUNTER: u32 = 0x00;
    /// Reset/reload value (32-bit)
    pub const RESET: u32 = 0x04;
    /// Match value 1 (32-bit)
    pub const MATCH1: u32 = 0x08;
    /// Match value 2 (32-bit)
    pub const MATCH2: u32 = 0x0C;
}

/// Global register offsets (at 0x30-0x3F)
mod global_regs {
    /// Global control (32-bit): 3 bits per timer for enable/clock/overflow-int
    pub const CONTROL: u32 = 0x30;
    /// Global status (32-bit): 9 bits for match1/match2/overflow per timer
    pub const STATUS: u32 = 0x34;
    /// Interrupt mask (32-bit)
    pub const MASK: u32 = 0x38;
    /// Revision (32-bit, read-only): 0x00010801
    pub const REVISION: u32 = 0x3C;
}

/// Global control register bit layout (CEmu: gpt.control)
/// Bits 0-2: Timer 1 (enable, clock_32k, overflow_int_enable)
/// Bits 3-5: Timer 2
/// Bits 6-8: Timer 3
/// Bits 9-11: Invert direction (count down) for timers 1-3
mod ctrl_bits {
    /// Timer 1 enable
    pub const TIMER1_ENABLE: u32 = 1 << 0;
    /// Timer 1 use 32KHz clock (vs CPU clock)
    pub const TIMER1_CLOCK_32K: u32 = 1 << 1;
    /// Timer 1 overflow interrupt enable
    pub const TIMER1_OVERFLOW_INT: u32 = 1 << 2;

    /// Timer 2 enable
    pub const TIMER2_ENABLE: u32 = 1 << 3;
    /// Timer 2 use 32KHz clock
    pub const TIMER2_CLOCK_32K: u32 = 1 << 4;
    /// Timer 2 overflow interrupt enable
    pub const TIMER2_OVERFLOW_INT: u32 = 1 << 5;

    /// Timer 3 enable
    pub const TIMER3_ENABLE: u32 = 1 << 6;
    /// Timer 3 use 32KHz clock
    pub const TIMER3_CLOCK_32K: u32 = 1 << 7;
    /// Timer 3 overflow interrupt enable
    pub const TIMER3_OVERFLOW_INT: u32 = 1 << 8;

    /// Timer 1 count down (invert direction)
    pub const TIMER1_COUNT_DOWN: u32 = 1 << 9;
    /// Timer 2 count down
    pub const TIMER2_COUNT_DOWN: u32 = 1 << 10;
    /// Timer 3 count down
    pub const TIMER3_COUNT_DOWN: u32 = 1 << 11;
}

/// Status register bit layout (9 bits total, 3 per timer)
/// For each timer: bit 0 = match1, bit 1 = match2, bit 2 = overflow
mod status_bits {
    /// Timer 1 match1 status
    pub const TIMER1_MATCH1: u32 = 1 << 0;
    /// Timer 1 match2 status
    pub const TIMER1_MATCH2: u32 = 1 << 1;
    /// Timer 1 overflow status
    pub const TIMER1_OVERFLOW: u32 = 1 << 2;

    /// Timer 2 match1 status
    pub const TIMER2_MATCH1: u32 = 1 << 3;
    /// Timer 2 match2 status
    pub const TIMER2_MATCH2: u32 = 1 << 4;
    /// Timer 2 overflow status
    pub const TIMER2_OVERFLOW: u32 = 1 << 5;

    /// Timer 3 match1 status
    pub const TIMER3_MATCH1: u32 = 1 << 6;
    /// Timer 3 match2 status
    pub const TIMER3_MATCH2: u32 = 1 << 7;
    /// Timer 3 overflow status
    pub const TIMER3_OVERFLOW: u32 = 1 << 8;
}

/// Hardware revision constant (from CEmu: gpt.revision = 0x00010801)
const REVISION_VALUE: u32 = 0x00010801;

/// Interrupt flags returned by tick()
pub mod interrupt {
    /// Interrupt triggered by match1 value
    pub const MATCH1: u8 = 1 << 0;
    /// Interrupt triggered by match2 value
    pub const MATCH2: u8 = 1 << 1;
    /// Interrupt triggered by zero/overflow
    pub const OVERFLOW: u8 = 1 << 2;
}

/// A single general-purpose timer's data registers
#[derive(Debug, Clone)]
pub struct TimerData {
    /// Current counter value
    counter: u32,
    /// Reset/reload value
    reset_value: u32,
    /// Match value 1
    match1: u32,
    /// Match value 2
    match2: u32,
}

impl TimerData {
    /// Create a new timer data block
    pub fn new() -> Self {
        Self {
            counter: 0,
            reset_value: 0,
            match1: 0,
            match2: 0,
        }
    }

    /// Reset to initial state
    pub fn reset(&mut self) {
        self.counter = 0;
        self.reset_value = 0;
        self.match1 = 0;
        self.match2 = 0;
    }

    /// Read a register byte (offset 0x00-0x0F)
    pub fn read(&self, offset: u32) -> u8 {
        let reg = offset & 0x0C;
        let byte_idx = (offset & 0x03) * 8;

        let value = match reg {
            regs::COUNTER => self.counter,
            regs::RESET => self.reset_value,
            regs::MATCH1 => self.match1,
            regs::MATCH2 => self.match2,
            _ => 0,
        };

        ((value >> byte_idx) & 0xFF) as u8
    }

    /// Write a register byte (offset 0x00-0x0F)
    pub fn write(&mut self, offset: u32, value: u8) {
        let reg = offset & 0x0C;
        let byte_idx = (offset & 0x03) * 8;
        let mask = 0xFF_u32 << byte_idx;
        let shifted = (value as u32) << byte_idx;

        match reg {
            regs::COUNTER => self.counter = (self.counter & !mask) | (shifted & mask),
            regs::RESET => self.reset_value = (self.reset_value & !mask) | (shifted & mask),
            regs::MATCH1 => self.match1 = (self.match1 & !mask) | (shifted & mask),
            regs::MATCH2 => self.match2 = (self.match2 & !mask) | (shifted & mask),
            _ => {}
        }
    }

    // Accessors for state persistence
    pub fn counter(&self) -> u32 {
        self.counter
    }
    pub fn reset_value(&self) -> u32 {
        self.reset_value
    }
    pub fn match1(&self) -> u32 {
        self.match1
    }
    pub fn match2(&self) -> u32 {
        self.match2
    }
    pub fn set_counter(&mut self, v: u32) {
        self.counter = v;
    }
    pub fn set_reset_value(&mut self, v: u32) {
        self.reset_value = v;
    }
    pub fn set_match1(&mut self, v: u32) {
        self.match1 = v;
    }
    pub fn set_match2(&mut self, v: u32) {
        self.match2 = v;
    }
}

impl Default for TimerData {
    fn default() -> Self {
        Self::new()
    }
}

/// The complete timer subsystem with 3 timers and global registers
#[derive(Debug, Clone)]
pub struct TimerSystem {
    /// Timer data registers (counter, reset, match1, match2)
    timers: [TimerData; 3],
    /// Global control register (enables, clock select, direction)
    control: u32,
    /// Global status register (match/overflow flags)
    status: u32,
    /// Interrupt mask
    mask: u32,
    /// Accumulated CPU cycles per timer (for clock division/32K timing)
    accum_cycles: [u64; 3],
}

impl TimerSystem {
    /// 32KHz crystal frequency
    const CLOCK_32K: u64 = 32768;

    /// Create a new timer system
    pub fn new() -> Self {
        Self {
            timers: [TimerData::new(), TimerData::new(), TimerData::new()],
            control: 0,
            status: 0,
            mask: 0,
            accum_cycles: [0; 3],
        }
    }

    /// Reset all timers and global registers
    pub fn reset(&mut self) {
        for timer in &mut self.timers {
            timer.reset();
        }
        self.control = 0;
        self.status = 0;
        self.mask = 0;
        self.accum_cycles = [0; 3];
    }

    /// Check if a specific timer is enabled (0-2)
    fn is_enabled(&self, idx: usize) -> bool {
        let shift = idx * 3;
        (self.control >> shift) & 1 != 0
    }

    /// Check if a timer uses 32KHz clock (vs CPU clock)
    fn uses_32k_clock(&self, idx: usize) -> bool {
        let shift = idx * 3 + 1;
        (self.control >> shift) & 1 != 0
    }

    /// Check if overflow interrupt is enabled for a timer
    fn overflow_int_enabled(&self, idx: usize) -> bool {
        let shift = idx * 3 + 2;
        (self.control >> shift) & 1 != 0
    }

    /// Check if timer counts down (inverted direction)
    fn counts_down(&self, idx: usize) -> bool {
        let shift = 9 + idx;
        (self.control >> shift) & 1 != 0
    }

    /// Tick all timers with given CPU cycles and CPU clock rate
    /// Returns a bitmask of which timers fired interrupts (bits 0-2 for timers 1-3)
    pub fn tick(&mut self, cpu_cycles: u32, cpu_clock: u64) -> u8 {
        let mut interrupts = 0u8;

        for idx in 0..3 {
            if !self.is_enabled(idx) {
                continue;
            }

            // Capture control bits before borrowing timer mutably
            let count_down = self.counts_down(idx);
            let overflow_int_enabled = self.overflow_int_enabled(idx);
            let uses_32k = self.uses_32k_clock(idx);

            // Accumulate cycles
            self.accum_cycles[idx] += cpu_cycles as u64;

            // Calculate effective ticks based on clock source
            let ticks = if uses_32k {
                // 32KHz clock: convert CPU cycles to 32K ticks
                let cycles_per_tick = cpu_clock / Self::CLOCK_32K;
                let t = self.accum_cycles[idx] / cycles_per_tick;
                self.accum_cycles[idx] %= cycles_per_tick;
                t as u32
            } else {
                // CPU clock: 1 tick per cycle
                let t = self.accum_cycles[idx] as u32;
                self.accum_cycles[idx] = 0;
                t
            };

            if ticks == 0 {
                continue;
            }

            let timer = &mut self.timers[idx];
            let old_counter = timer.counter;

            let mut status_bits = 0u32;

            if count_down {
                // Count down
                if timer.counter >= ticks {
                    timer.counter -= ticks;

                    // Check for match conditions
                    if Self::crosses_value_down(old_counter, timer.counter, timer.match1) {
                        status_bits |= 1 << 0; // match1
                    }
                    if Self::crosses_value_down(old_counter, timer.counter, timer.match2) {
                        status_bits |= 1 << 1; // match2
                    }
                } else {
                    // Underflow
                    // Check matches before underflow
                    if Self::crosses_value_down(old_counter, 0, timer.match1) {
                        status_bits |= 1 << 0;
                    }
                    if Self::crosses_value_down(old_counter, 0, timer.match2) {
                        status_bits |= 1 << 1;
                    }

                    // Overflow/underflow occurred
                    if overflow_int_enabled {
                        status_bits |= 1 << 2; // overflow
                    }

                    // Reload from reset value
                    let remaining = ticks - timer.counter;
                    timer.counter = timer.reset_value.wrapping_sub(remaining);
                }
            } else {
                // Count up
                let (new_val, overflow) = timer.counter.overflowing_add(ticks);
                timer.counter = new_val;

                // Check for match conditions
                if Self::crosses_value_up(old_counter, new_val, timer.match1, overflow) {
                    status_bits |= 1 << 0; // match1
                }
                if Self::crosses_value_up(old_counter, new_val, timer.match2, overflow) {
                    status_bits |= 1 << 1; // match2
                }

                if overflow {
                    // Overflow occurred
                    if overflow_int_enabled {
                        status_bits |= 1 << 2;
                    }
                    // Reload: counter already wrapped, add reset value
                    timer.counter = timer.reset_value.wrapping_add(new_val);
                }
            }

            // Update global status register (3 bits per timer)
            self.status |= status_bits << (idx * 3);

            // Check if any unmasked status bits are set for this timer
            let timer_status = (self.status >> (idx * 3)) & 0x7;
            let timer_mask = (self.mask >> (idx * 3)) & 0x7;
            if (timer_status & timer_mask) != 0 || status_bits != 0 {
                interrupts |= 1 << idx;
            }
        }

        interrupts
    }

    /// Check if a value was crossed when counting up
    fn crosses_value_up(old: u32, new: u32, target: u32, overflow: bool) -> bool {
        if overflow {
            target > old || target <= new
        } else {
            target > old && target <= new
        }
    }

    /// Check if a value was crossed when counting down
    fn crosses_value_down(old: u32, new: u32, target: u32) -> bool {
        target >= new && target < old
    }

    /// Read a byte from the timer address space (offset 0x00-0x3F)
    pub fn read(&self, offset: u32) -> u8 {
        if offset < 0x30 {
            // Timer data registers (0x10 bytes per timer)
            let timer_idx = (offset / 0x10) as usize;
            let reg_offset = offset % 0x10;
            if timer_idx < 3 {
                self.timers[timer_idx].read(reg_offset)
            } else {
                0
            }
        } else {
            // Global registers
            let byte_idx = (offset & 0x03) * 8;
            let value = match offset & 0x3C {
                0x30 => self.control,
                0x34 => self.status,
                0x38 => self.mask,
                0x3C => REVISION_VALUE,
                _ => 0,
            };
            ((value >> byte_idx) & 0xFF) as u8
        }
    }

    /// Write a byte to the timer address space (offset 0x00-0x3F)
    pub fn write(&mut self, offset: u32, value: u8) {
        if offset < 0x30 {
            // Timer data registers
            let timer_idx = (offset / 0x10) as usize;
            let reg_offset = offset % 0x10;
            if timer_idx < 3 {
                self.timers[timer_idx].write(reg_offset, value);
            }
        } else {
            let byte_idx = (offset & 0x03) * 8;
            let mask = 0xFF_u32 << byte_idx;
            let shifted = (value as u32) << byte_idx;

            match offset & 0x3C {
                0x30 => {
                    // Control register
                    self.control = (self.control & !mask) | (shifted & mask);
                }
                0x34 => {
                    // Status register - writing 1 clears bits (write-1-to-clear)
                    let clear_mask = shifted & 0x1FF; // Only 9 bits valid
                    self.status &= !clear_mask;
                }
                0x38 => {
                    // Mask register
                    self.mask = (self.mask & !mask) | (shifted & mask);
                }
                0x3C => {
                    // Revision is read-only
                }
                _ => {}
            }
        }
    }

    // ========== Compatibility with existing code ==========

    /// Get reference to timer 1 data
    pub fn timer1(&self) -> &TimerData {
        &self.timers[0]
    }

    /// Get reference to timer 2 data
    pub fn timer2(&self) -> &TimerData {
        &self.timers[1]
    }

    /// Get reference to timer 3 data
    pub fn timer3(&self) -> &TimerData {
        &self.timers[2]
    }

    /// Get mutable reference to timer 1 data
    pub fn timer1_mut(&mut self) -> &mut TimerData {
        &mut self.timers[0]
    }

    /// Get mutable reference to timer 2 data
    pub fn timer2_mut(&mut self) -> &mut TimerData {
        &mut self.timers[1]
    }

    /// Get mutable reference to timer 3 data
    pub fn timer3_mut(&mut self) -> &mut TimerData {
        &mut self.timers[2]
    }

    /// Get global control register value
    pub fn control(&self) -> u32 {
        self.control
    }

    /// Get global status register value
    pub fn status(&self) -> u32 {
        self.status
    }

    /// Get interrupt mask register value
    pub fn interrupt_mask(&self) -> u32 {
        self.mask
    }

    /// Set global control register (for state restoration)
    pub fn set_control(&mut self, v: u32) {
        self.control = v;
    }

    /// Set global status register (for state restoration)
    pub fn set_status(&mut self, v: u32) {
        self.status = v;
    }

    /// Set interrupt mask register (for state restoration)
    pub fn set_mask(&mut self, v: u32) {
        self.mask = v;
    }

    /// Get accumulated cycles for a timer (for state persistence)
    pub fn accum_cycles(&self, idx: usize) -> u64 {
        self.accum_cycles.get(idx).copied().unwrap_or(0)
    }

    /// Set accumulated cycles for a timer (for state restoration)
    pub fn set_accum_cycles(&mut self, idx: usize, v: u64) {
        if idx < 3 {
            self.accum_cycles[idx] = v;
        }
    }
}

impl Default for TimerSystem {
    fn default() -> Self {
        Self::new()
    }
}

// ========== Legacy Timer type for backward compatibility ==========

/// A single general-purpose timer (legacy interface)
/// This wraps TimerSystem for backward compatibility with existing code.
#[derive(Debug, Clone)]
pub struct Timer {
    /// Current counter value
    counter: u32,
    /// Reset/reload value
    reset_value: u32,
    /// Match value 1
    match1: u32,
    /// Match value 2
    match2: u32,
    /// Control register (legacy per-timer control byte)
    control: u8,
    /// Accumulated cycles (for clock division)
    accum_cycles: u32,
}

/// Control register bits (legacy per-timer control)
mod ctrl {
    /// Timer enable
    pub const ENABLE: u8 = 1 << 0;
    /// Count direction: 1=up, 0=down
    pub const COUNT_UP: u8 = 1 << 1;
    /// Interrupt on zero/overflow
    pub const INT_ON_ZERO: u8 = 1 << 2;
    /// Interrupt on match1
    pub const INT_ON_MATCH1: u8 = 1 << 3;
    /// Use reset value on overflow
    pub const USE_RESET: u8 = 1 << 4;
    /// Clock divider bits 5-7 (0=1, 1=2, 2=4, 3=8, ...)
    pub const CLOCK_DIV_SHIFT: u8 = 5;
    pub const CLOCK_DIV_MASK: u8 = 0x07;
}

impl Timer {
    /// Create a new timer
    pub fn new() -> Self {
        Self {
            counter: 0,
            reset_value: 0,
            match1: 0,
            match2: 0,
            control: 0,
            accum_cycles: 0,
        }
    }

    /// Reset the timer to initial state
    pub fn reset(&mut self) {
        self.counter = 0;
        self.reset_value = 0;
        self.match1 = 0;
        self.match2 = 0;
        self.control = 0;
        self.accum_cycles = 0;
    }

    /// Check if timer is enabled
    pub fn is_enabled(&self) -> bool {
        self.control & ctrl::ENABLE != 0
    }

    /// Current counter value
    pub fn counter(&self) -> u32 {
        self.counter
    }

    /// Reset/reload value
    pub fn reset_value(&self) -> u32 {
        self.reset_value
    }

    /// Match value 1
    pub fn match1(&self) -> u32 {
        self.match1
    }

    /// Match value 2
    pub fn match2(&self) -> u32 {
        self.match2
    }

    /// Control register
    pub fn control(&self) -> u8 {
        self.control
    }

    /// Check if counting up
    fn count_up(&self) -> bool {
        self.control & ctrl::COUNT_UP != 0
    }

    /// Check if interrupt on zero/overflow is enabled
    fn int_on_zero(&self) -> bool {
        self.control & ctrl::INT_ON_ZERO != 0
    }

    /// Check if interrupt on match1 is enabled
    fn int_on_match1(&self) -> bool {
        self.control & ctrl::INT_ON_MATCH1 != 0
    }

    /// Check if reset value should be used on overflow
    fn use_reset(&self) -> bool {
        self.control & ctrl::USE_RESET != 0
    }

    /// Get clock divider (1, 2, 4, 8, 16, 32, 64, 128)
    fn clock_divider(&self) -> u32 {
        let div_bits = (self.control >> ctrl::CLOCK_DIV_SHIFT) & ctrl::CLOCK_DIV_MASK;
        1 << div_bits
    }

    /// Tick the timer with given CPU cycles
    /// Returns interrupt flags (see `interrupt` module for bit definitions)
    /// Bit 0: match1, Bit 1: match2, Bit 2: zero/overflow
    pub fn tick(&mut self, cycles: u32) -> u8 {
        if !self.is_enabled() {
            return 0;
        }

        // Accumulate cycles and apply divider
        self.accum_cycles += cycles;
        let divider = self.clock_divider();
        let ticks = self.accum_cycles / divider;
        self.accum_cycles %= divider;

        if ticks == 0 {
            return 0;
        }

        let mut interrupts: u8 = 0;
        let old_counter = self.counter;

        if self.count_up() {
            // Count up
            let (new_val, overflow) = self.counter.overflowing_add(ticks);
            self.counter = new_val;

            // Check for match conditions when counting up
            // A match occurs if the counter value crosses or equals the match value
            if self.int_on_match1() {
                if Self::crosses_value_up(old_counter, new_val, self.match1, overflow) {
                    interrupts |= interrupt::MATCH1;
                }
            }
            // Match2 interrupt - always check (no separate enable bit in CEmu)
            if Self::crosses_value_up(old_counter, new_val, self.match2, overflow) {
                interrupts |= interrupt::MATCH2;
            }

            if overflow {
                // The wrapped value (new_val) represents ticks past the overflow point
                // After reload, continue counting up from reset_value
                if self.use_reset() {
                    self.counter = self.reset_value.wrapping_add(new_val);
                }
                if self.int_on_zero() {
                    interrupts |= interrupt::OVERFLOW;
                }
            }
        } else {
            // Count down
            if self.counter >= ticks {
                self.counter -= ticks;

                // Check for match conditions when counting down (no underflow case)
                if self.int_on_match1() {
                    if Self::crosses_value_down(old_counter, self.counter, self.match1) {
                        interrupts |= interrupt::MATCH1;
                    }
                }
                // Match2 interrupt - always check
                if Self::crosses_value_down(old_counter, self.counter, self.match2) {
                    interrupts |= interrupt::MATCH2;
                }
            } else {
                // Underflow: counter would go below 0
                // Check matches before underflow
                if self.int_on_match1() {
                    if Self::crosses_value_down(old_counter, 0, self.match1) {
                        interrupts |= interrupt::MATCH1;
                    }
                }
                if Self::crosses_value_down(old_counter, 0, self.match2) {
                    interrupts |= interrupt::MATCH2;
                }

                // Calculate how many ticks remain after reaching 0:
                // - It takes `counter` ticks to go from `counter` to 0
                // - Remaining ticks = ticks - counter
                // After reload, continue counting down from reset_value
                let remaining = ticks - self.counter;
                if self.use_reset() {
                    self.counter = self.reset_value.wrapping_sub(remaining);
                } else {
                    // Wrap around from 0xFFFFFFFF
                    self.counter = 0xFFFFFFFF_u32.wrapping_sub(remaining - 1);
                }
                if self.int_on_zero() {
                    interrupts |= interrupt::OVERFLOW;
                }
            }
        }

        interrupts
    }

    /// Check if a value was crossed when counting up (from old to new)
    /// Returns true if match_val is in range (old, new] or if overflow occurred and
    /// match_val is in the wrapped portion [0, new]
    fn crosses_value_up(old: u32, new: u32, match_val: u32, overflow: bool) -> bool {
        if overflow {
            // Counter wrapped around: check if match is in (old, MAX] or [0, new]
            match_val > old || match_val <= new
        } else {
            // Normal case: check if match is in range (old, new]
            match_val > old && match_val <= new
        }
    }

    /// Check if a value was crossed when counting down (from old to new)
    /// Returns true if match_val is in range [new, old)
    fn crosses_value_down(old: u32, new: u32, match_val: u32) -> bool {
        // Check if match is in range [new, old)
        match_val >= new && match_val < old
    }

    /// Read a register byte
    /// addr is offset within this timer (0-0x0F)
    pub fn read(&self, addr: u32) -> u8 {
        let reg = addr & 0x0C;
        let byte_offset = (addr & 0x03) * 8;

        let value = match reg {
            regs::COUNTER => self.counter,
            regs::RESET => self.reset_value,
            regs::MATCH1 => self.match1,
            regs::MATCH2 => self.match2,
            _ => 0,
        };

        ((value >> byte_offset) & 0xFF) as u8
    }

    /// Write a register byte
    /// addr is offset within this timer (0-0x0F)
    pub fn write(&mut self, addr: u32, value: u8) {
        let reg = addr & 0x0C;
        let byte_offset = (addr & 0x03) * 8;
        let mask = 0xFF_u32 << byte_offset;
        let shifted_value = (value as u32) << byte_offset;

        match reg {
            regs::COUNTER => {
                self.counter = (self.counter & !mask) | (shifted_value & mask);
            }
            regs::RESET => {
                self.reset_value = (self.reset_value & !mask) | (shifted_value & mask);
            }
            regs::MATCH1 => {
                self.match1 = (self.match1 & !mask) | (shifted_value & mask);
            }
            regs::MATCH2 => {
                self.match2 = (self.match2 & !mask) | (shifted_value & mask);
            }
            _ => {}
        }
    }

    /// Read control register
    pub fn read_control(&self) -> u8 {
        self.control
    }

    /// Write control register
    pub fn write_control(&mut self, value: u8) {
        self.control = value;
    }

    // ========== State Persistence ==========

    /// Get accumulated cycles
    pub fn accum_cycles(&self) -> u32 {
        self.accum_cycles
    }

    /// Set counter value directly
    pub fn set_counter(&mut self, value: u32) {
        self.counter = value;
    }

    /// Set reset value directly
    pub fn set_reset_value(&mut self, value: u32) {
        self.reset_value = value;
    }

    /// Set match1 value directly
    pub fn set_match1(&mut self, value: u32) {
        self.match1 = value;
    }

    /// Set match2 value directly
    pub fn set_match2(&mut self, value: u32) {
        self.match2 = value;
    }

    /// Set accumulated cycles directly
    pub fn set_accum_cycles(&mut self, value: u32) {
        self.accum_cycles = value;
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== TimerSystem tests ==========

    #[test]
    fn test_timer_system_new() {
        let ts = TimerSystem::new();
        assert_eq!(ts.control(), 0);
        assert_eq!(ts.status(), 0);
        assert_eq!(ts.interrupt_mask(), 0);
    }

    #[test]
    fn test_timer_system_reset() {
        let mut ts = TimerSystem::new();
        ts.write(0x00, 0x12); // Timer 1 counter
        ts.write(0x30, 0x01); // Enable timer 1
        ts.reset();
        assert_eq!(ts.read(0x00), 0);
        assert_eq!(ts.control(), 0);
    }

    #[test]
    fn test_global_control_read_write() {
        let mut ts = TimerSystem::new();

        // Write control register byte by byte
        ts.write(0x30, 0x49); // Byte 0: enable T1+T2, clock 32K for T2
        assert_eq!(ts.read(0x30), 0x49);
        assert_eq!(ts.control(), 0x49);

        ts.write(0x31, 0x02); // Byte 1: count down for T1
        assert_eq!(ts.read(0x31), 0x02);
        assert_eq!(ts.control(), 0x0249);
    }

    #[test]
    fn test_global_status_read_write() {
        let mut ts = TimerSystem::new();

        // Status starts at 0
        assert_eq!(ts.status(), 0);

        // Enable timer 1 counting up
        ts.write(0x30, ctrl_bits::TIMER1_ENABLE as u8);

        // Set counter near overflow
        ts.write(0x03, 0xFF);
        ts.write(0x02, 0xFF);
        ts.write(0x01, 0xFF);
        ts.write(0x00, 0xFF);

        // Enable overflow interrupt
        ts.write(0x30, (ctrl_bits::TIMER1_ENABLE | ctrl_bits::TIMER1_OVERFLOW_INT) as u8);

        // Tick to cause overflow
        ts.tick(2, 48_000_000);

        // Status should have overflow bit set
        assert_ne!(ts.status() & status_bits::TIMER1_OVERFLOW, 0);

        // Write 1 to clear status
        ts.write(0x34, status_bits::TIMER1_OVERFLOW as u8);
        assert_eq!(ts.status() & status_bits::TIMER1_OVERFLOW, 0);
    }

    #[test]
    fn test_global_mask_read_write() {
        let mut ts = TimerSystem::new();

        ts.write(0x38, 0x07); // Enable all timer 1 interrupts
        assert_eq!(ts.read(0x38), 0x07);
        assert_eq!(ts.interrupt_mask(), 0x07);
    }

    #[test]
    fn test_revision_read_only() {
        let mut ts = TimerSystem::new();

        // Read revision
        assert_eq!(ts.read(0x3C), 0x01); // LSB of 0x00010801
        assert_eq!(ts.read(0x3D), 0x08);
        assert_eq!(ts.read(0x3E), 0x01);
        assert_eq!(ts.read(0x3F), 0x00); // MSB

        // Try to write - should be ignored
        ts.write(0x3C, 0xFF);
        assert_eq!(ts.read(0x3C), 0x01);
    }

    #[test]
    fn test_timer_data_routing() {
        let mut ts = TimerSystem::new();

        // Write to timer 1 counter (offset 0x00-0x03)
        ts.write(0x00, 0x12);
        ts.write(0x01, 0x34);
        ts.write(0x02, 0x56);
        ts.write(0x03, 0x78);
        assert_eq!(ts.timer1().counter(), 0x78563412);

        // Write to timer 2 counter (offset 0x10-0x13)
        ts.write(0x10, 0xAB);
        assert_eq!(ts.timer2().counter(), 0x000000AB);

        // Write to timer 3 reset value (offset 0x24-0x27)
        ts.write(0x24, 0xCD);
        assert_eq!(ts.timer3().reset_value(), 0x000000CD);
    }

    #[test]
    fn test_timer_count_up() {
        let mut ts = TimerSystem::new();

        // Enable timer 1 counting up (bit 9 = 0 means count up)
        ts.write(0x30, ctrl_bits::TIMER1_ENABLE as u8);
        ts.write(0x00, 0x00); // Counter = 0

        // Tick 100 cycles
        ts.tick(100, 48_000_000);

        assert_eq!(ts.timer1().counter(), 100);
    }

    #[test]
    fn test_timer_count_down() {
        let mut ts = TimerSystem::new();

        // Enable timer 1 counting down
        ts.write(0x30, ctrl_bits::TIMER1_ENABLE as u8);
        ts.write(0x31, (ctrl_bits::TIMER1_COUNT_DOWN >> 8) as u8); // Set count down bit

        // Set counter to 1000
        ts.write(0x00, 0xE8); // 1000 = 0x3E8
        ts.write(0x01, 0x03);

        // Tick 100 cycles
        ts.tick(100, 48_000_000);

        assert_eq!(ts.timer1().counter(), 900);
    }

    #[test]
    fn test_timer_32k_clock() {
        let mut ts = TimerSystem::new();

        // Enable timer 1 with 32KHz clock
        ts.write(0x30, (ctrl_bits::TIMER1_ENABLE | ctrl_bits::TIMER1_CLOCK_32K) as u8);

        // At 48MHz CPU clock, cycles_per_32k_tick = 48_000_000 / 32768 = 1464
        // Tick 1464 cycles should yield 1 timer tick
        ts.tick(1464, 48_000_000);
        assert_eq!(ts.timer1().counter(), 1);

        // Tick another 1000 cycles - shouldn't be enough for another tick
        ts.tick(1000, 48_000_000);
        assert_eq!(ts.timer1().counter(), 1);

        // Tick 464 more cycles to complete another tick
        ts.tick(464, 48_000_000);
        assert_eq!(ts.timer1().counter(), 2);
    }

    #[test]
    fn test_match_status() {
        let mut ts = TimerSystem::new();

        // Enable timer 1
        ts.write(0x30, ctrl_bits::TIMER1_ENABLE as u8);

        // Set match1 to 50
        ts.write(0x08, 50);

        // Counter starts at 0, tick 50 cycles
        ts.tick(50, 48_000_000);

        // Match1 should be set in status
        assert_ne!(ts.status() & status_bits::TIMER1_MATCH1, 0);
    }

    #[test]
    fn test_multiple_timers() {
        let mut ts = TimerSystem::new();

        // Enable all 3 timers
        ts.write(0x30, (ctrl_bits::TIMER1_ENABLE | ctrl_bits::TIMER2_ENABLE | ctrl_bits::TIMER3_ENABLE) as u8);

        // Set different initial counters
        ts.write(0x00, 10); // Timer 1 = 10
        ts.write(0x10, 20); // Timer 2 = 20
        ts.write(0x20, 30); // Timer 3 = 30

        // Tick 5 cycles
        ts.tick(5, 48_000_000);

        // All timers should have incremented by 5
        assert_eq!(ts.timer1().counter(), 15);
        assert_eq!(ts.timer2().counter(), 25);
        assert_eq!(ts.timer3().counter(), 35);
    }

    #[test]
    fn test_overflow_with_reset() {
        let mut ts = TimerSystem::new();

        // Enable timer 1 with overflow interrupt
        ts.write(0x30, (ctrl_bits::TIMER1_ENABLE | ctrl_bits::TIMER1_OVERFLOW_INT) as u8);

        // Set reset value to 1000
        ts.write(0x04, 0xE8);
        ts.write(0x05, 0x03);

        // Set counter near overflow
        ts.write(0x00, 0xFE);
        ts.write(0x01, 0xFF);
        ts.write(0x02, 0xFF);
        ts.write(0x03, 0xFF);

        // Tick 3 cycles (should overflow and reload)
        let irq = ts.tick(3, 48_000_000);
        assert_ne!(irq, 0); // Should have interrupt

        // Counter should be reset_value + (overflow amount - 1)
        // 0xFFFFFFFE + 3 = 0x100000001, wrapped = 1, so counter = 1000 + 1 = 1001
        assert_eq!(ts.timer1().counter(), 1001);
    }

    // ========== Legacy Timer tests (preserved from original) ==========

    #[test]
    fn test_new() {
        let timer = Timer::new();
        assert!(!timer.is_enabled());
        assert_eq!(timer.counter, 0);
    }

    #[test]
    fn test_reset() {
        let mut timer = Timer::new();
        timer.counter = 0x12345678;
        timer.reset_value = 0x1000;
        timer.control = ctrl::ENABLE | ctrl::COUNT_UP;
        timer.accum_cycles = 100;

        timer.reset();
        assert_eq!(timer.counter, 0);
        assert_eq!(timer.reset_value, 0);
        assert_eq!(timer.control, 0);
        assert_eq!(timer.accum_cycles, 0);
        assert!(!timer.is_enabled());
    }

    #[test]
    fn test_disabled_timer_no_tick() {
        let mut timer = Timer::new();
        timer.counter = 100;
        // Timer is disabled by default

        let irq = timer.tick(50);
        assert_eq!(irq, 0);
        assert_eq!(timer.counter, 100); // Should not change
    }

    #[test]
    fn test_count_up() {
        let mut timer = Timer::new();
        timer.control = ctrl::ENABLE | ctrl::COUNT_UP;
        timer.counter = 0;

        let irq = timer.tick(100);
        assert_eq!(irq, 0);
        assert_eq!(timer.counter, 100);
    }

    #[test]
    fn test_count_down() {
        let mut timer = Timer::new();
        timer.control = ctrl::ENABLE;
        timer.counter = 100;

        let irq = timer.tick(50);
        assert_eq!(irq, 0);
        assert_eq!(timer.counter, 50);
    }

    #[test]
    fn test_underflow_interrupt() {
        let mut timer = Timer::new();
        timer.control = ctrl::ENABLE | ctrl::INT_ON_ZERO;
        timer.counter = 5;

        // Tick more than counter value should underflow
        let irq = timer.tick(10);
        assert!(irq & interrupt::OVERFLOW != 0);
    }

    #[test]
    fn test_underflow_with_reset() {
        let mut timer = Timer::new();
        timer.control = ctrl::ENABLE | ctrl::USE_RESET;
        timer.counter = 5;
        timer.reset_value = 1000;

        // Underflow should reload from reset value and continue counting
        // 5 ticks to reach 0, then reload to 1000, then 5 more ticks
        // Expected: 1000 - 5 = 995
        timer.tick(10);
        assert_eq!(timer.counter, 995);
    }

    #[test]
    fn test_underflow_exact_boundary() {
        let mut timer = Timer::new();
        timer.control = ctrl::ENABLE | ctrl::USE_RESET;
        timer.counter = 5;
        timer.reset_value = 1000;

        // 6 ticks: 5->4->3->2->1->0->(reload to 1000)->999
        timer.tick(6);
        assert_eq!(timer.counter, 999);
    }

    #[test]
    fn test_underflow_no_reset_wraps() {
        let mut timer = Timer::new();
        timer.control = ctrl::ENABLE; // No USE_RESET flag
        timer.counter = 5;

        // 6 ticks: 5->4->3->2->1->0->(wrap to 0xFFFFFFFF)
        timer.tick(6);
        assert_eq!(timer.counter, 0xFFFFFFFF);

        // Reset and try 10 ticks: wraps to 0xFFFFFFFF then counts down 4 more
        timer.counter = 5;
        timer.tick(10);
        assert_eq!(timer.counter, 0xFFFFFFFF - 4); // 0xFFFFFFFB
    }

    #[test]
    fn test_overflow_interrupt() {
        let mut timer = Timer::new();
        timer.control = ctrl::ENABLE | ctrl::COUNT_UP | ctrl::INT_ON_ZERO;
        timer.counter = 0xFFFFFFFF;

        let irq = timer.tick(2);
        assert_ne!(irq & interrupt::OVERFLOW, 0);
    }

    #[test]
    fn test_overflow_no_interrupt_when_disabled() {
        let mut timer = Timer::new();
        timer.control = ctrl::ENABLE | ctrl::COUNT_UP; // INT_ON_ZERO not set
        timer.counter = 0xFFFFFFFF;
        timer.match2 = 0xFFFFFFFE; // Avoid match2 crossing on overflow

        let irq = timer.tick(2);
        assert_eq!(irq, 0); // No interrupt because INT_ON_ZERO is not set
    }

    #[test]
    fn test_clock_divider() {
        let mut timer = Timer::new();
        timer.control = ctrl::ENABLE | ctrl::COUNT_UP | (2 << ctrl::CLOCK_DIV_SHIFT);
        timer.counter = 0;

        // Divider is 4, so 8 cycles = 2 ticks
        let irq = timer.tick(8);
        assert_eq!(irq, 0);
        assert_eq!(timer.counter, 2);
    }

    #[test]
    fn test_clock_divider_accumulation() {
        let mut timer = Timer::new();
        timer.control = ctrl::ENABLE | ctrl::COUNT_UP | (3 << ctrl::CLOCK_DIV_SHIFT);
        // Divider is 8

        // Tick 3 cycles - not enough for a tick
        timer.tick(3);
        assert_eq!(timer.counter, 0);

        // Tick 5 more = 8 total, should get 1 tick
        timer.tick(5);
        assert_eq!(timer.counter, 1);

        // Tick 17 more = 2 ticks with 1 leftover
        timer.tick(17);
        assert_eq!(timer.counter, 3);
    }

    #[test]
    fn test_clock_divider_max() {
        let mut timer = Timer::new();
        // Max divider bits = 7, so divider = 1 << 7 = 128
        timer.control = ctrl::ENABLE | ctrl::COUNT_UP | (7 << ctrl::CLOCK_DIV_SHIFT);
        timer.counter = 0;

        // 128 cycles = 1 tick
        timer.tick(128);
        assert_eq!(timer.counter, 1);

        // 127 cycles = 0 ticks (accumulated)
        timer.tick(127);
        assert_eq!(timer.counter, 1);

        // 1 more cycle = another tick (128 accumulated)
        timer.tick(1);
        assert_eq!(timer.counter, 2);
    }

    #[test]
    fn test_underflow_counter_zero_ticks_one() {
        let mut timer = Timer::new();
        timer.control = ctrl::ENABLE | ctrl::USE_RESET;
        timer.counter = 0;
        timer.reset_value = 1000;

        // counter=0, ticks=1: underflow immediately
        // remaining = 1 - 0 = 1, counter = 1000 - 1 = 999
        timer.tick(1);
        assert_eq!(timer.counter, 999);
    }

    #[test]
    fn test_underflow_counter_zero_no_reset() {
        let mut timer = Timer::new();
        timer.control = ctrl::ENABLE; // No USE_RESET
        timer.counter = 0;

        // counter=0, ticks=1: wraps to 0xFFFFFFFF - (1-1) = 0xFFFFFFFF
        timer.tick(1);
        assert_eq!(timer.counter, 0xFFFFFFFF);
    }

    #[test]
    fn test_read_write_counter() {
        let mut timer = Timer::new();

        timer.write(0, 0x12);
        timer.write(1, 0x34);
        timer.write(2, 0x56);
        timer.write(3, 0x78);

        assert_eq!(timer.counter, 0x78563412);
        assert_eq!(timer.read(0), 0x12);
        assert_eq!(timer.read(1), 0x34);
        assert_eq!(timer.read(2), 0x56);
        assert_eq!(timer.read(3), 0x78);
    }

    #[test]
    fn test_reset_on_overflow() {
        let mut timer = Timer::new();
        timer.control = ctrl::ENABLE | ctrl::COUNT_UP | ctrl::USE_RESET;
        timer.counter = 0xFFFFFFFE;
        timer.reset_value = 0x1000;

        // Trace: 0xFFFFFFFE -> 0xFFFFFFFF -> 0x00000000 (overflow, reload to 0x1000) -> 0x1001
        // 2 ticks to overflow, then reload to reset_value, then 1 more tick
        // Expected: 0x1000 + 1 = 0x1001
        timer.tick(3);
        assert_eq!(timer.counter, 0x1001);
    }

    #[test]
    fn test_overflow_exact_boundary() {
        let mut timer = Timer::new();
        timer.control = ctrl::ENABLE | ctrl::COUNT_UP | ctrl::USE_RESET;
        timer.counter = 0xFFFFFFFE;
        timer.reset_value = 0x1000;

        // Trace: 0xFFFFFFFE -> 0xFFFFFFFF -> 0x00000000 (overflow, reload to 0x1000)
        // Exactly 2 ticks to overflow, wrapped value is 0, so counter = reset_value + 0
        timer.tick(2);
        assert_eq!(timer.counter, 0x1000);
    }
}
