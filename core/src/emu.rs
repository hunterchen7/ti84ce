//! Emulator orchestrator
//!
//! Coordinates the CPU, bus, and peripherals to run the TI-84 Plus CE.

use crate::bus::{Bus, IoRecord};
use crate::cpu::{Cpu, InterruptMode};
use crate::peripherals::rtc::LATCH_TICK_OFFSET;
use crate::scheduler::{EventId, Scheduler};
use std::os::raw::c_char;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};

/// Zero-cost logging macro — compiles to nothing in WASM builds.
/// Use this instead of `log_event(&format!(...))` to avoid format string
/// allocation overhead in WASM where logging is a no-op.
#[cfg(not(target_arch = "wasm32"))]
macro_rules! log_evt {
    ($($arg:tt)*) => {
        $crate::emu::log_event(&format!($($arg)*))
    };
}

#[cfg(target_arch = "wasm32")]
macro_rules! log_evt {
    ($($arg:tt)*) => { /* no-op in WASM */ };
}

pub(crate) use log_evt;

/// Instruction trace flag - when enabled, logs every instruction
static INST_TRACE_ENABLED: AtomicBool = AtomicBool::new(false);
/// Number of instructions traced (resets when trace is enabled)
static INST_TRACE_COUNT: AtomicU32 = AtomicU32::new(0);
/// Maximum instructions to trace before auto-disable (0 = unlimited)
static INST_TRACE_LIMIT: AtomicU32 = AtomicU32::new(0);
/// Armed trace - will enable when CPU wakes from HALT
static INST_TRACE_ARMED: AtomicBool = AtomicBool::new(false);
/// Limit for armed trace
static INST_TRACE_ARMED_LIMIT: AtomicU32 = AtomicU32::new(0);

/// Enable instruction tracing (logs every instruction to log callback)
#[allow(dead_code)]
pub fn enable_inst_trace(limit: u32) {
    INST_TRACE_COUNT.store(0, Ordering::SeqCst);
    INST_TRACE_LIMIT.store(limit, Ordering::SeqCst);
    INST_TRACE_ENABLED.store(true, Ordering::SeqCst);
    log_evt!("INST_TRACE: enabled, limit={}", limit);
}

/// Arm instruction tracing to start when CPU wakes from HALT
/// This avoids tracing HALT loops - only traces after wake event
#[allow(dead_code)]
pub fn arm_inst_trace_on_wake(limit: u32) {
    INST_TRACE_ARMED_LIMIT.store(limit, Ordering::SeqCst);
    INST_TRACE_ARMED.store(true, Ordering::SeqCst);
    log_evt!("INST_TRACE: armed for wake, limit={}", limit);
}

/// Disable instruction tracing
#[allow(dead_code)]
pub fn disable_inst_trace() {
    INST_TRACE_ENABLED.store(false, Ordering::SeqCst);
    INST_TRACE_ARMED.store(false, Ordering::SeqCst);
    log_evt!("INST_TRACE: disabled");
}

/// Check if instruction tracing is enabled
#[allow(dead_code)]
pub fn is_inst_trace_enabled() -> bool {
    INST_TRACE_ENABLED.load(Ordering::Relaxed)
}

/// Check and trigger armed trace on wake
fn check_armed_trace_on_wake(was_halted: bool, is_halted: bool) {
    // If we were halted and now we're not, trigger the armed trace
    if was_halted && !is_halted && INST_TRACE_ARMED.load(Ordering::SeqCst) {
        let limit = INST_TRACE_ARMED_LIMIT.load(Ordering::SeqCst);
        INST_TRACE_ARMED.store(false, Ordering::SeqCst);
        INST_TRACE_COUNT.store(0, Ordering::SeqCst);
        INST_TRACE_LIMIT.store(limit, Ordering::SeqCst);
        INST_TRACE_ENABLED.store(true, Ordering::SeqCst);
        log_evt!("INST_TRACE: triggered on wake, limit={}", limit);
    }
}

/// TI-84 Plus CE screen dimensions
pub const SCREEN_WIDTH: usize = 320;
pub const SCREEN_HEIGHT: usize = 240;

/// Cycles after which boot is considered complete and TI-OS initialization can happen
/// Boot completes at ~62M cycles; we wait a bit longer to ensure TI-OS is ready
const BOOT_COMPLETE_CYCLES: u64 = 65_000_000;

/// TI-OS Automatic Power Down (APD) disable address.
///
/// On a real TI-84 CE, the OS puts the device to sleep after ~5 minutes of
/// inactivity. In the emulator this is undesirable because:
///   1. The host device (phone/browser) has its own power/sleep management
///   2. Without proper APD handling, the emulator flashes or becomes unresponsive
///
/// The OS stores APD control in the `apdFlags` byte at (flags + 0x08) = 0xD00088.
/// Bit 2 (`apdAble`) enables the APD countdown timer. Clearing it prevents the OS
/// from ever triggering inactivity sleep, while still allowing manual power-off
/// via 2nd+ON (which writes POWER register bit 6 directly).
///
/// Reference: TI-84 CE SDK — flags = 0xD00080, apdFlags = flags + 0x08
const APD_FLAGS_ADDR: u32 = 0xD00088;
const APD_ABLE_BIT: u8 = 2;

/// Number of entries in the PC/opcode history ring buffer
const HISTORY_SIZE: usize = 64;

/// Single entry in the execution history
#[derive(Clone, Copy, Default)]
struct HistoryEntry {
    /// Program counter before instruction
    pc: u32,
    /// Opcode byte(s) - up to 4 bytes for prefixed instructions
    opcode: [u8; 4],
    /// Number of valid opcode bytes
    opcode_len: u8,
}

/// Execution history ring buffer for crash diagnostics
struct ExecutionHistory {
    /// Ring buffer of history entries
    entries: [HistoryEntry; HISTORY_SIZE],
    /// Write index (next position to write)
    write_idx: usize,
    /// Number of entries written (max HISTORY_SIZE)
    count: usize,
}

impl ExecutionHistory {
    fn new() -> Self {
        Self {
            entries: [HistoryEntry::default(); HISTORY_SIZE],
            write_idx: 0,
            count: 0,
        }
    }

    /// Record an instruction execution
    fn record(&mut self, pc: u32, opcode: &[u8]) {
        let mut entry = HistoryEntry {
            pc,
            opcode: [0; 4],
            opcode_len: opcode.len().min(4) as u8,
        };
        for (i, &byte) in opcode.iter().take(4).enumerate() {
            entry.opcode[i] = byte;
        }
        self.entries[self.write_idx] = entry;
        self.write_idx = (self.write_idx + 1) % HISTORY_SIZE;
        if self.count < HISTORY_SIZE {
            self.count += 1;
        }
    }

    /// Get history entries in execution order (oldest to newest)
    fn iter(&self) -> impl Iterator<Item = &HistoryEntry> {
        let start = if self.count < HISTORY_SIZE {
            0
        } else {
            self.write_idx
        };
        (0..self.count).map(move |i| {
            let idx = (start + i) % HISTORY_SIZE;
            &self.entries[idx]
        })
    }

    fn clear(&mut self) {
        self.write_idx = 0;
        self.count = 0;
    }
}

static LOG_CALLBACK: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(ptr::null_mut());

pub(crate) fn set_log_callback(cb: Option<extern "C" fn(*const c_char)>) {
    let ptr = cb.map(|f| f as *mut std::ffi::c_void).unwrap_or(ptr::null_mut());
    LOG_CALLBACK.store(ptr, Ordering::SeqCst);
}

/// Public logging function for use by other modules.
/// In WASM builds this is a no-op (callback is never set).
#[cfg(not(target_arch = "wasm32"))]
pub fn log_event(message: &str) {
    let cb_ptr = LOG_CALLBACK.load(Ordering::SeqCst);
    if !cb_ptr.is_null() {
        let cb: extern "C" fn(*const c_char) = unsafe { std::mem::transmute(cb_ptr) };
        if let Ok(cstr) = std::ffi::CString::new(message) {
            cb(cstr.as_ptr());
        }
        return;
    }

    // Fallback: append to emu.log
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open("emu.log") {
        let _ = std::io::Write::write_fmt(&mut file, format_args!("{message}\n"));
    }
}

#[cfg(target_arch = "wasm32")]
#[inline(always)]
pub fn log_event(_message: &str) {
    // No-op in WASM — but callers still evaluate format!() args.
    // Use log_event_fmt!() macro instead for zero-cost in WASM.
}

/// Reason for stopping execution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// Completed requested cycles
    CyclesComplete,
    /// CPU halted (HALT instruction)
    Halted,
    // TODO: Wire up UnimplementedOpcode when CPU reports unimplemented instructions (Milestone 5+)
    /// Unimplemented opcode encountered
    UnimplementedOpcode(u8),
    // TODO: Wire up BusFault when Bus reports invalid memory access (Milestone 5+)
    /// Bus fault (invalid memory access)
    BusFault(u32),
}

/// Information about a single instruction step (for trace comparison)
/// Captures state BEFORE execution to match CEmu's trace format
#[derive(Debug, Clone)]
pub struct StepInfo {
    /// PC before instruction execution
    pub pc: u32,
    /// SP before execution
    pub sp: u32,
    /// A register before execution
    pub a: u8,
    /// F (flags) register before execution
    pub f: u8,
    /// BC register before execution
    pub bc: u32,
    /// DE register before execution
    pub de: u32,
    /// HL register before execution
    pub hl: u32,
    /// IX register before execution
    pub ix: u32,
    /// IY register before execution
    pub iy: u32,
    /// ADL mode before execution
    pub adl: bool,
    /// IFF1 (interrupt flip-flop 1) before execution
    pub iff1: bool,
    /// IFF2 (interrupt flip-flop 2) before execution
    pub iff2: bool,
    /// Interrupt mode before execution
    pub im: InterruptMode,
    /// Whether CPU was halted before this step
    pub halted: bool,
    /// Opcode bytes at PC (up to 4 bytes)
    pub opcode: [u8; 4],
    /// Number of valid opcode bytes
    pub opcode_len: usize,
    /// Cycles used by this instruction
    pub cycles: u32,
    /// Total cycles after this instruction
    pub total_cycles: u64,
    /// I/O operations performed by this instruction (when full trace enabled)
    pub io_ops: Vec<IoRecord>,
}

/// Main emulator state
pub struct Emu {
    /// eZ80 CPU
    cpu: Cpu,
    /// System bus (memory, I/O)
    bus: Bus,
    /// Event scheduler for timed events
    scheduler: Scheduler,

    /// Framebuffer in ARGB8888 format
    framebuffer: Vec<u32>,

    /// ROM loaded flag
    rom_loaded: bool,

    /// Calculator is powered on (ON key was pressed)
    /// CPU won't execute until this is true
    powered_on: bool,

    /// Execution history for crash diagnostics
    history: ExecutionHistory,

    /// Last stop reason
    last_stop: StopReason,

    /// Total cycles executed
    total_cycles: u64,
    /// Whether we've already logged a HALT state
    halt_logged: bool,
    /// Whether TI-OS expression parser has been initialized after boot
    /// See docs/findings.md "TI-OS Expression Parser Requires Initialization After Boot"
    boot_init_done: bool,
    /// Frame counter for periodic diagnostic logging
    #[cfg(not(target_arch = "wasm32"))]
    frame_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimerSnapshot {
    pub counter: u32,
    pub reset_value: u32,
    pub match1: u32,
    pub match2: u32,
    pub control: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LcdSnapshot {
    pub timing: [u32; 4],
    pub control: u32,
    pub int_mask: u32,
    pub int_status: u32,
    pub upbase: u32,
    pub lpbase: u32,
    pub compare_state: u8,
}

impl Emu {
    /// Create a new emulator instance
    pub fn new() -> Self {
        Self {
            cpu: Cpu::new(),
            bus: Bus::new(),
            scheduler: Scheduler::new(),
            framebuffer: vec![0xFF000000; SCREEN_WIDTH * SCREEN_HEIGHT],
            rom_loaded: false,
            powered_on: false,
            history: ExecutionHistory::new(),
            last_stop: StopReason::CyclesComplete,
            total_cycles: 0,
            halt_logged: false,
            boot_init_done: false,
            #[cfg(not(target_arch = "wasm32"))]
            frame_count: 0,
        }
    }

    /// Load ROM data into flash
    pub fn load_rom(&mut self, data: &[u8]) -> Result<(), i32> {
        if data.is_empty() {
            return Err(-2); // Empty ROM
        }

        self.bus.load_rom(data).map_err(|_| -3)?; // -3 = ROM too large
        self.rom_loaded = true;
        log_evt!("ROM_LOADED bytes={}", data.len());
        self.reset();
        Ok(())
    }

    /// Set serial flash mode
    /// - true: Serial flash (newer TI-84 CE models) - uses cache timing
    /// - false: Parallel flash (older models) - uses constant 10 cycle timing
    pub fn set_serial_flash(&mut self, enabled: bool) {
        self.bus.set_serial_flash(enabled);
    }

    /// Get serial flash mode
    pub fn is_serial_flash(&self) -> bool {
        self.bus.is_serial_flash()
    }

    /// Reset emulator to initial state
    pub fn reset(&mut self) {
        log_evt!("RESET");
        self.cpu.reset();
        self.bus.reset();
        self.scheduler.reset();
        self.history.clear();
        self.last_stop = StopReason::CyclesComplete;
        self.total_cycles = 0;
        self.halt_logged = false;
        self.boot_init_done = false;
        self.powered_on = false; // Require ON key press to power on again
        // Initialize CPU prefetch buffer - charges cycles for first instruction's first byte
        // This matches CEmu's cpu_inst_start() call at the beginning of cpu_execute()
        self.cpu.init_prefetch(&mut self.bus);

        // Account for init_prefetch cycles in total_cycles for trace parity with CEmu
        // CEmu's cycle counter includes the prefetch cost before the first instruction
        self.total_cycles = self.bus.total_cycles();

        // Sync scheduler with initial cycles before scheduling RTC
        self.scheduler.advance(self.total_cycles);

        // Initialize RTC 1-second cycle from boot
        // CEmu's rtc_reset() does: sched_repeat_relative(SCHED_RTC, SCHED_SECOND, 0, LATCH_TICK_OFFSET)
        // This schedules the RTC LATCH event to fire at LATCH_TICK_OFFSET (16429) ticks after
        // each second boundary. We start from time 0, so first LATCH is at LATCH_TICK_OFFSET.
        self.scheduler.set(EventId::Rtc, LATCH_TICK_OFFSET);

        // Clear framebuffer to black
        for pixel in &mut self.framebuffer {
            *pixel = 0xFF000000;
        }
    }

    /// Run for specified cycles, returns cycles actually executed
    ///
    /// # TI-OS Expression Parser Initialization
    ///
    /// Note: Parser initialization is NOT handled here. Instead, it's handled in `set_key()`
    /// which auto-injects an ENTER on the first key press after boot. This allows the boot
    /// screen ("TI-84 Plus CE", OS version, "RAM Cleared") to remain visible until the user
    /// presses their first key. See `set_key()` documentation for details.
    pub fn run_cycles(&mut self, cycles: u32) -> u32 {
        if !self.rom_loaded || !self.powered_on || self.is_off() {
            return 0;
        }

        // Sync check: bus.cycles should match total_cycles
        debug_assert_eq!(self.total_cycles, self.bus.total_cycles(),
            "total_cycles desync: emu={} bus={}", self.total_cycles, self.bus.total_cycles());

        let mut cycles_remaining = cycles as i32;
        let start_cycles = self.total_cycles;

        while cycles_remaining > 0 {
            // Sync scheduler with CPU speed setting
            let cpu_speed = self.bus.ports.control.cpu_speed();
            self.scheduler.set_cpu_speed(cpu_speed);

            // Record PC and peek at opcode before execution
            let pc = self.cpu.pc;
            let (opcode, opcode_len) = self.peek_opcode(pc);
            let was_halted = self.cpu.halted;

            // Instruction tracing (when enabled via FFI, not in WASM)
            #[cfg(not(target_arch = "wasm32"))]
            if INST_TRACE_ENABLED.load(Ordering::Relaxed) && !self.cpu.halted {
                let count = INST_TRACE_COUNT.fetch_add(1, Ordering::Relaxed);
                let limit = INST_TRACE_LIMIT.load(Ordering::Relaxed);

                let opcode_str: String = opcode[..opcode_len]
                    .iter()
                    .map(|b| format!("{:02X}", b))
                    .collect::<Vec<_>>()
                    .join(" ");

                log_evt!(
                    "INST[{}]: PC={:06X} OP={} A={:02X} F={:02X} BC={:06X} DE={:06X} HL={:06X} SP={:06X} halted={} wake={}",
                    count, pc, opcode_str,
                    self.cpu.a, self.cpu.f,
                    self.cpu.bc, self.cpu.de, self.cpu.hl,
                    self.cpu.sp(),
                    self.cpu.halted, self.cpu.any_key_wake
                );

                if limit > 0 && count >= limit {
                    INST_TRACE_ENABLED.store(false, Ordering::SeqCst);
                    log_evt!("INST_TRACE: auto-disabled after limit reached");
                }
            }

            // Handle CPU_SIGNAL_ANY_KEY equivalent - call any_key_check before CPU executes
            if self.cpu.any_key_wake {
                let key_state = self.bus.key_state().clone();
                let should_interrupt = self.bus.ports.keypad.any_key_check(&key_state);
                if should_interrupt {
                    use crate::peripherals::interrupt::sources;
                    self.bus.ports.interrupt.raise(sources::KEYPAD);
                }
            }

            // Execute one instruction
            let cycles_used = self.cpu.step(&mut self.bus);

            // Check for wake event - triggers armed trace if CPU woke from HALT
            check_armed_trace_on_wake(was_halted, self.cpu.halted);

            // Record in history
            self.history.record(pc, &opcode[..opcode_len]);

            // Advance scheduler with cycles used at current speed, THEN handle speed change
            cycles_remaining -= cycles_used as i32;
            self.scheduler.advance(cycles_used as u64);

            // Check for CPU speed change AFTER advancing scheduler
            let new_cpu_speed = self.bus.ports.control.cpu_speed();
            if new_cpu_speed != cpu_speed {
                let old_mhz = match cpu_speed { 0 => 6, 1 => 12, 2 => 24, _ => 48 };
                let new_mhz = match new_cpu_speed { 0 => 6, 1 => 12, 2 => 24, _ => 48 };
                self.scheduler.convert_cpu_events(new_mhz, old_mhz);
                self.scheduler.set_cpu_speed(new_cpu_speed);
            }

            // Sync total_cycles with bus (handles speed conversion)
            self.total_cycles = self.bus.total_cycles();

            // Process pending scheduler events
            self.process_scheduler_events();

            // DMA cycle stealing: if LCD DMA consumed bus time, steal CPU cycles
            let dma_stolen = self.process_dma_stealing();
            if dma_stolen > 0 {
                cycles_remaining -= dma_stolen as i32;
            }

            // Check if SPI needs initial scheduling (state changed via port write)
            if self.bus.take_spi_schedule_flag() && !self.scheduler.is_active(EventId::Spi) {
                if let Some(ticks) = self.bus.spi().try_start_transfer_for_scheduler() {
                    self.scheduler.set(EventId::Spi, ticks);
                }
            }

            // Check for NMI from memory protection violations
            if self.bus.take_nmi_flag() {
                self.cpu.nmi_pending = true;
            }

            // Tick peripherals and check for interrupts
            if self.tick_peripherals(cycles_used) {
                self.cpu.irq_pending = true;
            }

            // Stop if device went off (OS wrote POWER bit 6 during this instruction)
            if self.is_off() {
                break;
            }

            // CEmu HALT fast-forward: when halted, advance cycles to next scheduled event.
            // This matches CEmu's cpu_halt() which sets cpu.cycles = cpu.next.
            // We must do this AFTER processing scheduler events above, so we know what's next.
            //
            // Performance: LCD DMA fires every ~19 cycles at Clock48M, creating thousands
            // of events per frame. Instead of returning to the outer loop for each event
            // (which requires cpu.step + tick_peripherals overhead), we use a tight inner
            // loop that only processes scheduler events and DMA stealing. Peripheral ticks
            // (OS Timer, keypad, etc.) are batched every HALT_TICK_BATCH cycles.
            if self.cpu.halted {
                self.last_stop = StopReason::Halted;
                const HALT_TICK_BATCH: u64 = 10_000;
                let mut peripheral_debt: u64 = 0;

                loop {
                    // Stop if device went off during this frame (OS wrote POWER bit 6)
                    if self.is_off() { break; }

                    let skip = self.scheduler.cycles_until_next_event();
                    if skip == 0 {
                        if !self.cpu.iff1 && !self.cpu.nmi_pending {
                            log_evt!(
                                "HALT_STUCK: pc={:06X} iff1={} iff2={} irq={} nmi={} cycles_left={} total={}",
                                self.cpu.pc, self.cpu.iff1, self.cpu.iff2,
                                self.cpu.irq_pending, self.cpu.nmi_pending,
                                cycles_remaining, self.total_cycles
                            );
                            break; // Nothing can wake the CPU
                        }

                        // Process any pending events first (e.g. LcdDma in the
                        // past after DMA stealing advanced base_ticks). Without
                        // this, the batch path would advance base_ticks further
                        // without processing the pending event, growing the gap
                        // and causing a DMA catch-up storm on HALT exit.
                        self.process_scheduler_events();
                        let dma_stolen = self.process_dma_stealing();
                        if dma_stolen > 0 {
                            cycles_remaining -= dma_stolen as i32;
                            peripheral_debt += dma_stolen;
                        }

                        // Check if processing events made a future event available
                        let new_skip = self.scheduler.cycles_until_next_event();
                        if new_skip > 0 {
                            continue; // Retry with the new skip value
                        }

                        // Genuinely no events — batch advance so tick_peripherals
                        // can generate an OS Timer interrupt to wake the CPU.
                        // Cap at SCHED_SECOND boundary to prevent process_second()
                        // from saturating event timestamps to 0 (causes DMA catch-up storm).
                        let to_sched_second = self.scheduler.cycles_until_sched_second();
                        let batch = HALT_TICK_BATCH
                            .min(to_sched_second.max(1))
                            .min(cycles_remaining.max(0) as u64);
                        if batch == 0 { break; }
                        self.bus.add_cycles(batch);
                        cycles_remaining -= batch as i32;
                        self.scheduler.advance(batch);
                        self.total_cycles = self.bus.total_cycles();
                        if self.tick_peripherals(batch as u32) {
                            self.cpu.irq_pending = true;
                            break; // Interrupt will wake CPU on next step()
                        }
                        peripheral_debt = 0; // batch already ticked peripherals
                        continue;
                    }

                    let skip = skip.min(cycles_remaining.max(0) as u64);
                    if skip == 0 { break; }

                    self.bus.add_cycles(skip);
                    cycles_remaining -= skip as i32;
                    self.scheduler.advance(skip);
                    self.total_cycles = self.bus.total_cycles();

                    // Process events (DMA, LCD state machine, timers, etc.)
                    self.process_scheduler_events();

                    // DMA cycle stealing
                    let dma_stolen = self.process_dma_stealing();
                    if dma_stolen > 0 {
                        cycles_remaining -= dma_stolen as i32;
                    }

                    peripheral_debt += skip + dma_stolen;

                    // Periodically tick peripherals (OS Timer, keypad, etc.)
                    if peripheral_debt >= HALT_TICK_BATCH {
                        if self.tick_peripherals(peripheral_debt as u32) {
                            self.cpu.irq_pending = true;
                        }
                        peripheral_debt = 0;
                    }

                    // Check wake conditions
                    if self.cpu.irq_pending && self.cpu.iff1 { break; }
                    if self.cpu.nmi_pending { break; }
                    if cycles_remaining <= 0 { break; }
                }

                // Flush any remaining peripheral debt
                if peripheral_debt > 0 {
                    if self.tick_peripherals(peripheral_debt as u32) {
                        self.cpu.irq_pending = true;
                    }
                }
            }
        }

        self.last_stop = StopReason::CyclesComplete;
        let executed = (self.total_cycles - start_cycles) as u32;

        // Periodic frame diagnostic logging (non-WASM only)
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.frame_count += 1;
            // Detect stuck-in-ISR: low PC, iff1=false, not halted, actively running
            let is_stuck_isr = !self.cpu.halted && !self.cpu.iff1 && self.cpu.iff2
                && self.cpu.pc < 0x002000;
            // Log every 60 frames (~1 second) or when CPU is stuck
            let is_stuck_halt = self.cpu.halted && !self.cpu.iff1;
            if self.frame_count % 60 == 0 || is_stuck_halt || is_stuck_isr {
                let active_events = self.scheduler.active_event_names();
                let pending_irqs = self.bus.ports.interrupt.pending_source_names();
                let raw_irqs = self.bus.ports.interrupt.raw();
                let status_irqs = self.bus.ports.interrupt.status();
                let enabled_irqs = self.bus.ports.interrupt.enabled();
                log_evt!(
                    "FRAME[{}]: pc={:06X} halted={} iff1={} iff2={} irq={} nmi={} executed={}/{} total={} events=[{}] pending=[{}] raw={:05X} status={:05X} enabled={:05X} SP={:06X}",
                    self.frame_count, self.cpu.pc,
                    self.cpu.halted, self.cpu.iff1, self.cpu.iff2,
                    self.cpu.irq_pending, self.cpu.nmi_pending,
                    executed, cycles, self.total_cycles,
                    active_events, pending_irqs,
                    raw_irqs, status_irqs, enabled_irqs,
                    self.cpu.sp()
                );
            }
            // One-shot: dump execution history when first entering stuck-ISR state
            if is_stuck_isr && self.frame_count > 120 {
                // Only dump once (use halt_logged as a one-shot flag)
                if !self.halt_logged {
                    self.halt_logged = true;
                    log_evt!("STUCK_ISR_HISTORY: {}", self.dump_history());
                    log_evt!("STUCK_ISR_REGS: {}", self.dump_registers());
                }
            }
        }

        executed
    }

    /// Internal run_cycles without boot initialization check (to avoid recursion)
    fn run_cycles_internal(&mut self, cycles: u32) -> u32 {
        if !self.rom_loaded || !self.powered_on {
            return 0;
        }

        let mut cycles_remaining = cycles as i32;
        let start_cycles = self.total_cycles;

        while cycles_remaining > 0 {
            let cpu_speed = self.bus.ports.control.cpu_speed();
            self.scheduler.set_cpu_speed(cpu_speed);

            let was_halted = self.cpu.halted;
            let cycles_used = self.cpu.step(&mut self.bus);
            check_armed_trace_on_wake(was_halted, self.cpu.halted);

            // Advance scheduler with cycles used at current speed, then handle speed change
            cycles_remaining -= cycles_used as i32;
            self.scheduler.advance(cycles_used as u64);

            let new_cpu_speed = self.bus.ports.control.cpu_speed();
            if new_cpu_speed != cpu_speed {
                let old_mhz = match cpu_speed { 0 => 6, 1 => 12, 2 => 24, _ => 48 };
                let new_mhz = match new_cpu_speed { 0 => 6, 1 => 12, 2 => 24, _ => 48 };
                self.scheduler.convert_cpu_events(new_mhz, old_mhz);
                self.scheduler.set_cpu_speed(new_cpu_speed);
            }

            self.total_cycles = self.bus.total_cycles();
            self.process_scheduler_events();

            // DMA cycle stealing
            let dma_stolen = self.process_dma_stealing();
            if dma_stolen > 0 {
                cycles_remaining -= dma_stolen as i32;
            }

            // Check if SPI needs initial scheduling (state changed via port write)
            if self.bus.take_spi_schedule_flag() && !self.scheduler.is_active(EventId::Spi) {
                if let Some(ticks) = self.bus.spi().try_start_transfer_for_scheduler() {
                    self.scheduler.set(EventId::Spi, ticks);
                }
            }

            // Check for NMI from memory protection violations
            if self.bus.take_nmi_flag() {
                self.cpu.nmi_pending = true;
            }

            if self.tick_peripherals(cycles_used) {
                self.cpu.irq_pending = true;
            }

            // HALT fast-forward (same batched approach as run_cycles)
            if self.cpu.halted {
                const HALT_TICK_BATCH: u64 = 10_000;
                let mut peripheral_debt: u64 = 0;

                loop {
                    let skip = self.scheduler.cycles_until_next_event();
                    if skip == 0 {
                        if !self.cpu.iff1 && !self.cpu.nmi_pending { break; }
                        let batch = HALT_TICK_BATCH.min(cycles_remaining.max(0) as u64);
                        if batch == 0 { break; }
                        self.bus.add_cycles(batch);
                        cycles_remaining -= batch as i32;
                        self.scheduler.advance(batch);
                        self.total_cycles = self.bus.total_cycles();
                        if self.tick_peripherals(batch as u32) {
                            self.cpu.irq_pending = true;
                            break;
                        }
                        continue;
                    }

                    let skip = skip.min(cycles_remaining.max(0) as u64);
                    if skip == 0 { break; }

                    self.bus.add_cycles(skip);
                    cycles_remaining -= skip as i32;
                    self.scheduler.advance(skip);
                    self.total_cycles = self.bus.total_cycles();
                    self.process_scheduler_events();

                    let dma_stolen = self.process_dma_stealing();
                    if dma_stolen > 0 {
                        cycles_remaining -= dma_stolen as i32;
                    }

                    peripheral_debt += skip + dma_stolen;

                    if peripheral_debt >= HALT_TICK_BATCH {
                        if self.tick_peripherals(peripheral_debt as u32) {
                            self.cpu.irq_pending = true;
                        }
                        peripheral_debt = 0;
                    }

                    if self.cpu.irq_pending && self.cpu.iff1 { break; }
                    if self.cpu.nmi_pending { break; }
                    if cycles_remaining <= 0 { break; }
                }

                if peripheral_debt > 0 {
                    if self.tick_peripherals(peripheral_debt as u32) {
                        self.cpu.irq_pending = true;
                    }
                }
            }
        }

        (self.total_cycles - start_cycles) as u32
    }

    /// Execute exactly one instruction and return detailed step information.
    ///
    /// This captures state BEFORE execution to match CEmu's trace format.
    /// Use this for accurate trace comparison with CEmu.
    pub fn step(&mut self) -> Option<StepInfo> {
        if !self.rom_loaded || !self.powered_on {
            return None;
        }

        // Sync scheduler with CPU speed setting
        let cpu_speed = self.bus.ports.control.cpu_speed();
        self.scheduler.set_cpu_speed(cpu_speed);

        // Capture state BEFORE execution
        let pc = self.cpu.pc;
        let sp = self.cpu.sp();
        let a = self.cpu.a;
        let f = self.cpu.f;
        let bc = self.cpu.bc;
        let de = self.cpu.de;
        let hl = self.cpu.hl;
        let ix = self.cpu.ix;
        let iy = self.cpu.iy;
        let adl = self.cpu.adl;
        let iff1 = self.cpu.iff1;
        let iff2 = self.cpu.iff2;
        let im = self.cpu.im;
        let was_halted = self.cpu.halted;

        // Read opcode bytes at PC
        let (opcode, opcode_len) = self.peek_opcode(pc);

        // Clear I/O ops buffer and set instruction context for tracing
        self.bus.clear_instruction_io_ops();
        self.bus.set_instruction_context(pc, &opcode[..opcode_len]);

        // Handle CPU_SIGNAL_ANY_KEY equivalent
        if self.cpu.any_key_wake {
            log_evt!("ANY_KEY_CHECK: mode={} halted={} iff1={}",
                self.bus.ports.keypad.mode(), self.cpu.halted, self.cpu.iff1);
            let key_state = self.bus.key_state().clone();
            let should_interrupt = self.bus.ports.keypad.any_key_check(&key_state);
            if should_interrupt {
                log_evt!("ANY_KEY_CHECK: raising keypad interrupt");
                use crate::peripherals::interrupt::sources;
                self.bus.ports.interrupt.raise(sources::KEYPAD);
            }
        }

        // Execute one instruction
        let cycles_used = self.cpu.step(&mut self.bus);

        // Check for wake event
        check_armed_trace_on_wake(was_halted, self.cpu.halted);

        // Record in history
        self.history.record(pc, &opcode[..opcode_len]);

        // Advance scheduler with cycles used at current speed, then handle speed change
        self.scheduler.advance(cycles_used as u64);

        let new_cpu_speed = self.bus.ports.control.cpu_speed();
        if new_cpu_speed != cpu_speed {
            let old_mhz = match cpu_speed { 0 => 6, 1 => 12, 2 => 24, _ => 48 };
            let new_mhz = match new_cpu_speed { 0 => 6, 1 => 12, 2 => 24, _ => 48 };
            self.scheduler.convert_cpu_events(new_mhz, old_mhz);
            self.scheduler.set_cpu_speed(new_cpu_speed);
        }

        self.total_cycles = self.bus.total_cycles();

        // Process pending scheduler events
        self.process_scheduler_events();

        // DMA cycle stealing
        self.process_dma_stealing();

        // Check if SPI needs initial scheduling (state changed via port write)
        if self.bus.take_spi_schedule_flag() && !self.scheduler.is_active(EventId::Spi) {
            if let Some(ticks) = self.bus.spi().try_start_transfer_for_scheduler() {
                self.scheduler.set(EventId::Spi, ticks);
            }
        }

        // Check for NMI from memory protection violations
        if self.bus.take_nmi_flag() {
            self.cpu.nmi_pending = true;
        }

        // Tick peripherals and check for interrupts
        if self.tick_peripherals(cycles_used) {
            self.cpu.irq_pending = true;
        }

        // HALT fast-forward: advance to next scheduled event (batched for DMA efficiency)
        if self.cpu.halted {
            self.last_stop = StopReason::Halted;
            const HALT_TICK_BATCH: u64 = 10_000;
            const STEP_HALT_CAP: u64 = 10_000_000;
            let mut total_advanced: u64 = 0;
            let mut peripheral_debt: u64 = 0;

            loop {
                let skip = self.scheduler.cycles_until_next_event();
                if skip == 0 {
                    if !self.cpu.iff1 && !self.cpu.nmi_pending { break; }
                    let batch = HALT_TICK_BATCH.min(STEP_HALT_CAP - total_advanced);
                    if batch == 0 { break; }
                    self.bus.add_cycles(batch);
                    self.scheduler.advance(batch);
                    self.total_cycles = self.bus.total_cycles();
                    total_advanced += batch;
                    if self.tick_peripherals(batch as u32) {
                        self.cpu.irq_pending = true;
                        break;
                    }
                    continue;
                }

                let skip = skip.min(STEP_HALT_CAP - total_advanced);
                if skip == 0 { break; }

                self.bus.add_cycles(skip);
                self.scheduler.advance(skip);
                self.total_cycles = self.bus.total_cycles();
                total_advanced += skip;
                self.process_scheduler_events();
                self.process_dma_stealing();

                peripheral_debt += skip;
                if peripheral_debt >= HALT_TICK_BATCH {
                    if self.tick_peripherals(peripheral_debt as u32) {
                        self.cpu.irq_pending = true;
                    }
                    peripheral_debt = 0;
                }

                if self.cpu.irq_pending && self.cpu.iff1 { break; }
                if self.cpu.nmi_pending { break; }
                if total_advanced >= STEP_HALT_CAP { break; }
            }

            if peripheral_debt > 0 {
                if self.tick_peripherals(peripheral_debt as u32) {
                    self.cpu.irq_pending = true;
                }
            }
        }

        // Collect I/O ops from this instruction
        let io_ops = self.bus.take_instruction_io_ops();

        Some(StepInfo {
            pc,
            sp,
            a,
            f,
            bc,
            de,
            hl,
            ix,
            iy,
            adl,
            iff1,
            iff2,
            im,
            halted: was_halted,
            opcode,
            opcode_len,
            cycles: cycles_used,
            total_cycles: self.total_cycles,
            io_ops,
        })
    }

    /// Tick peripherals and handle timer delay pipeline scheduling.
    /// Returns true if any interrupt is pending.
    fn tick_peripherals(&mut self, cycles: u32) -> bool {
        // Get timer delay remaining for the delay pipeline packing
        let delay_remaining = self.scheduler.ticks_remaining(EventId::TimerDelay);
        let irq = self.bus.ports.tick(cycles, delay_remaining);

        // If timer tick generated new delay pipeline data, schedule the TimerDelay event
        if self.bus.ports.timers.needs_delay_event {
            self.bus.ports.timers.needs_delay_event = false;
            if !self.scheduler.is_active(EventId::TimerDelay) {
                self.scheduler.set(EventId::TimerDelay, 2);
            }
        }

        // Check LCD scheduling flags (set by control register writes)
        if self.bus.ports.lcd.needs_lcd_event {
            self.bus.ports.lcd.needs_lcd_event = false;
            // LCD just enabled — schedule immediate LCD event (sched_set(SCHED_LCD, 0))
            self.scheduler.set(EventId::Lcd, 0);
        }
        if self.bus.ports.lcd.needs_lcd_clear {
            self.bus.ports.lcd.needs_lcd_clear = false;
            // LCD just disabled — clear LCD and LCD DMA events
            self.scheduler.clear(EventId::Lcd);
            self.scheduler.clear(EventId::LcdDma);
        }

        irq
    }

    /// Process any pending scheduler events
    fn process_scheduler_events(&mut self) {
        use crate::peripherals::interrupt::sources;

        // Process all pending events
        while let Some(event) = self.scheduler.next_pending_event() {
            match event {
                EventId::Rtc => {
                    // Process RTC event using 3-state machine (TICK/LATCH/LOAD_LATCH)
                    let (next_delay, raise_interrupt) = self.bus.ports.rtc.process_event();
                    if raise_interrupt {
                        // TODO: Wire RTC interrupt to interrupt controller
                        // CEmu: intrpt_set(INT_RTC, true) — INT_RTC is a dedicated line
                    }
                    // Schedule next RTC event
                    self.scheduler.repeat(EventId::Rtc, next_delay);
                }
                EventId::Spi => {
                    // SPI transfer complete - process and maybe start next
                    if let Some(ticks) = self.bus.spi().complete_transfer_and_continue() {
                        // Another transfer started, reschedule
                        self.scheduler.repeat(EventId::Spi, ticks);
                    } else {
                        // No more transfers pending
                        self.scheduler.clear(EventId::Spi);
                    }
                }
                EventId::TimerDelay => {
                    // Timer 2-cycle delay pipeline: process one tier of deferred interrupts
                    let (_status, _intrpt, has_more) = self.bus.ports.timers.process_delay();
                    // Update interrupt controller based on effective (status & mask)
                    // This matches CEmu: intrpt_set(INT_TIMERn, (status & mask) & bits)
                    let int_state = self.bus.ports.timers.interrupt_state();
                    let timer_sources = [sources::TIMER1, sources::TIMER2, sources::TIMER3];
                    for (i, &src) in timer_sources.iter().enumerate() {
                        if int_state & (1 << i) != 0 {
                            self.bus.ports.interrupt.raise(src);
                        } else {
                            self.bus.ports.interrupt.clear_raw(src);
                        }
                    }
                    self.cpu.irq_pending = self.bus.ports.interrupt.irq_pending();
                    if has_more {
                        // More tiers pending — reschedule for 1 more CPU cycle
                        self.scheduler.set(EventId::TimerDelay, 1);
                    } else {
                        self.scheduler.clear(EventId::TimerDelay);
                    }
                }
                EventId::Timer0 | EventId::Timer1 | EventId::Timer2 => {
                    // Timer fired — update all timer interrupts based on (status & mask)
                    let int_state = self.bus.ports.timers.interrupt_state();
                    let timer_sources = [sources::TIMER1, sources::TIMER2, sources::TIMER3];
                    for (i, &src) in timer_sources.iter().enumerate() {
                        if int_state & (1 << i) != 0 {
                            self.bus.ports.interrupt.raise(src);
                        } else {
                            self.bus.ports.interrupt.clear_raw(src);
                        }
                    }
                    self.cpu.irq_pending = self.bus.ports.interrupt.irq_pending();
                    self.scheduler.clear(event);
                }
                EventId::OsTimer => {
                    // OS Timer fired - raise OSTIMER interrupt
                    self.bus.ports.interrupt.raise(sources::OSTIMER);
                    self.cpu.irq_pending = self.bus.ports.interrupt.irq_pending();
                    // OS Timer auto-repeats, reschedule it
                    // Note: the actual period is set by the timer peripheral
                    self.scheduler.clear(EventId::OsTimer);
                }
                EventId::Lcd => {
                    // LCD event state machine — matches CEmu's lcd_event()
                    let result = self.bus.ports.lcd.process_event();
                    // Update interrupt controller based on lcd.ris & lcd.imsc
                    if result.interrupt_changed {
                        if self.bus.ports.lcd.check_interrupt() {
                            self.bus.ports.interrupt.raise(sources::LCD);
                        } else {
                            self.bus.ports.interrupt.clear_raw(sources::LCD);
                        }
                        self.cpu.irq_pending = self.bus.ports.interrupt.irq_pending();
                    }
                    // Schedule DMA if needed (relative to this LCD event)
                    if let Some(offset) = result.schedule_dma_offset {
                        self.scheduler.repeat_relative(
                            EventId::LcdDma, EventId::Lcd, offset, 0,
                        );
                    }
                    // Reschedule LCD event
                    self.scheduler.repeat(EventId::Lcd, result.duration);
                }
                EventId::LcdDma => {
                    // LCD DMA — reads VRAM and advances UPCURR.
                    // DMA consumes bus time tracked via dma_last_mem_timestamp.
                    // CEmu: last_mem_timestamp += callback.dma(id) * tick_unit
                    let result = self.bus.ports.lcd.process_dma();
                    let tick_unit = crate::scheduler::ClockId::Clock48M
                        .base_ticks_per_tick(self.scheduler.cpu_speed());
                    if let Some(ticks) = result.repeat_ticks {
                        self.scheduler.dma_last_mem_timestamp += ticks * tick_unit;
                        let skipped = self.scheduler.repeat_catchup(EventId::LcdDma, ticks);
                        if skipped > 0 {
                            // Fast-forward LCD DMA state for the skipped events
                            // (advances cur_col, cur_row, upcurr in O(1)).
                            // Do NOT add dma_last_mem_timestamp for skipped events:
                            // the bus contention didn't actually happen, and adding
                            // it would cause process_dma_stealing to advance base_ticks
                            // past the rescheduled event, creating a feedback loop.
                            self.bus.ports.lcd.fast_forward_dma_events(skipped);
                        }
                    } else if let Some(offset) = result.schedule_relative {
                        // Schedule relative to LCD event
                        self.scheduler.repeat_relative(
                            EventId::LcdDma, EventId::Lcd, offset, 0,
                        );
                    } else {
                        self.scheduler.clear(EventId::LcdDma);
                    }
                }
                _ => {
                    // Unknown event - clear it
                    self.scheduler.clear(event);
                }
            }
        }
    }

    /// Process DMA cycle stealing.
    ///
    /// CEmu's `sched_process_pending_dma()` checks if DMA has consumed bus time
    /// ahead of the CPU. If so, CPU cycles are "stolen" by advancing the cycle
    /// counter. This is called after `process_scheduler_events()` at instruction
    /// boundaries.
    ///
    /// Returns the number of CPU cycles stolen by DMA.
    fn process_dma_stealing(&mut self) -> u64 {
        // Sync DMA timestamp to CPU time if behind (CEmu initializes to cpu_timestamp)
        if self.scheduler.dma_last_mem_timestamp < self.scheduler.base_ticks {
            self.scheduler.dma_last_mem_timestamp = self.scheduler.base_ticks;
        }

        let stolen = if self.scheduler.dma_last_mem_timestamp > self.scheduler.base_ticks {
            // DMA is ahead of CPU — steal cycles
            // CEmu: cpu.dmaCycles += div_ceil(last_mem_timestamp, cpu_clock) - cpu.cycles
            let ahead_base_ticks =
                self.scheduler.dma_last_mem_timestamp - self.scheduler.base_ticks;
            let stolen_cycles =
                self.scheduler.base_ticks_to_cpu_cycles_ceil(ahead_base_ticks);
            if stolen_cycles > 0 {
                self.scheduler.advance(stolen_cycles);
                self.bus.add_cycles(stolen_cycles);
                self.total_cycles = self.bus.total_cycles();
            }
            stolen_cycles
        } else {
            0
        };

        // Reset DMA timestamp to current CPU time (CEmu: last_mem = cpu_timestamp after access)
        self.scheduler.dma_last_mem_timestamp = self.scheduler.base_ticks;

        stolen
    }

    /// Peek at opcode bytes at address without affecting state
    /// Returns (bytes, length) to avoid heap allocation in hot loop
    fn peek_opcode(&mut self, addr: u32) -> ([u8; 4], usize) {
        let mut bytes = [0u8; 4];
        let first = self.bus.peek_byte(addr);
        bytes[0] = first;

        // Check for prefix bytes
        let len = match first {
            0xCB | 0xED => {
                bytes[1] = self.bus.peek_byte(addr.wrapping_add(1));
                2
            }
            0xDD | 0xFD => {
                let second = self.bus.peek_byte(addr.wrapping_add(1));
                bytes[1] = second;
                if second == 0xCB {
                    bytes[2] = self.bus.peek_byte(addr.wrapping_add(2));
                    bytes[3] = self.bus.peek_byte(addr.wrapping_add(3));
                    4
                } else {
                    2
                }
            }
            _ => 1,
        };

        (bytes, len)
    }

    /// Get framebuffer dimensions
    pub fn framebuffer_size(&self) -> (usize, usize) {
        (SCREEN_WIDTH, SCREEN_HEIGHT)
    }

    /// Get raw pointer to framebuffer
    pub fn framebuffer_ptr(&self) -> *const u32 {
        self.framebuffer.as_ptr()
    }

    /// Get framebuffer as a slice (safe access)
    pub fn framebuffer_data(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Set key state
    /// Special handling for ON key (row 2, col 0) which has dedicated interrupt
    /// Set key state in the keypad matrix.
    ///
    /// # TI-OS Expression Parser Initialization
    ///
    /// On first key press after boot completes, this automatically injects an ENTER
    /// press/release to dismiss the boot screen and initialize the TI-OS expression parser.
    ///
    /// **Behavior:**
    /// 1. On boot, TI-OS displays "TI-84 Plus CE", OS version, and "RAM Cleared"
    /// 2. User's first key press is detected (any key except ON, any time after boot)
    /// 3. If first key is ENTER: just process it (dismisses boot screen + inits parser)
    /// 4. If first key is ON: ignore it (keep waiting for a normal key)
    /// 5. If first key is any other key: inject ENTER first, then process user's key
    ///
    /// **Why this is needed:**
    /// After boot, TI-OS expression parser is in an uninitialized state. The first ENTER
    /// press shows "Done" instead of evaluating expressions. Subsequent ENTERs work normally.
    /// By auto-injecting ENTER on first user interaction, we:
    /// - Let users see the boot screen (OS version info)
    /// - Initialize the parser transparently
    /// - Provide smooth UX (no need to manually press ENTER twice)
    ///
    /// See docs/findings.md "TI-OS Expression Parser Requires Initialization After Boot"
    pub fn set_key(&mut self, row: usize, col: usize, down: bool) {
        // Auto-initialize TI-OS parser on first key press after boot
        // Skip ON key (row 2, col 0) - it's for power management, not normal input
        if down && !self.boot_init_done && self.total_cycles > BOOT_COMPLETE_CYCLES && !(row == 2 && col == 0) {
            // If user's first key IS ENTER, just let it through (don't inject another ENTER)
            // Otherwise, inject ENTER before processing their key
            if row == 6 && col == 0 {
                log_evt!("BOOT_INIT: first key is ENTER, using it to dismiss boot screen");
                self.boot_init_done = true;
                self.disable_apd();
                // Continue to process user's ENTER press below
            } else {
                log_evt!("BOOT_INIT: first key press detected, auto-dismissing boot screen with ENTER");
                // Press ENTER (row 6, col 0) to dismiss boot screen
                self.bus.set_key(6, 0, true);
                self.cpu.any_key_wake = true;
                self.run_cycles_internal(1_500_000);
                // Release ENTER
                self.bus.set_key(6, 0, false);
                self.run_cycles_internal(3_000_000);
                self.boot_init_done = true;
                self.disable_apd();
                log_evt!("BOOT_INIT: boot screen dismissed, processing user key");
                // Continue to process the original key press below
            }
        }

        // ON key (row 2, col 0) has special handling - it can wake from HALT
        // even with interrupts disabled and raises dedicated ON_KEY interrupt
        if row == 2 && col == 0 {
            if down {
                self.press_on_key();
            } else {
                self.release_on_key();
            }
        } else {
            // Set key state and trigger any_key_check (like CEmu's CPU_SIGNAL_ANY_KEY handling)
            // This updates keypad data registers
            self.bus.set_key(row, col, down);

            // Set any_key_wake signal to wake CPU from HALT
            // This allows keys to wake the CPU so the OS can poll the keypad
            if down {
                self.cpu.any_key_wake = true;
            }
        }
    }

    /// Disable TI-OS Automatic Power Down (APD) by clearing the `apdAble` flag
    /// in the OS system flags area. See APD_FLAGS_ADDR constant for details.
    fn disable_apd(&mut self) {
        let flags = self.bus.peek_byte(APD_FLAGS_ADDR);
        self.bus.poke_byte(APD_FLAGS_ADDR, flags & !(1 << APD_ABLE_BIT));
        log_evt!("APD disabled: apdFlags 0x{:02X} -> 0x{:02X}", flags, flags & !(1 << APD_ABLE_BIT));
    }

    /// Get the backlight brightness level (0-255).
    /// Returns 0 when backlight is off (screen should appear black).
    pub fn get_backlight(&self) -> u8 {
        self.bus.ports.backlight.brightness()
    }

    /// Check if LCD is on (should display content).
    /// Returns true when both conditions are met:
    /// 1. Control port 0x05 bit 4 is set (lcd_flag_enabled)
    /// 2. LCD controller bit 11 is set (lcd powered)
    /// This matches CEmu's lcdwidget.cpp check for "LCD OFF" display.
    pub fn is_lcd_on(&self) -> bool {
        self.bus.ports.control.lcd_flag_enabled() && self.bus.ports.lcd.is_powered()
    }

    /// Check if the device is in the "off" (sleep) state.
    /// The OS writes bit 6 of the POWER register to enter sleep mode.
    /// The device stays off until an ON key press triggers a WAKE interrupt.
    pub fn is_off(&self) -> bool {
        self.bus.ports.control.is_off()
    }

    /// Press the ON key - wakes CPU from HALT even with interrupts disabled
    /// Also raises the ON_KEY and WAKE interrupts for normal interrupt handling
    ///
    /// Matches CEmu's keypad_on_check(): when device is off, pulses INT_WAKE
    /// and clears control.off. The pulse (set then clear) handles the case where
    /// the OS has configured WAKE as inverted — the clear step sets the status bit.
    /// wake() also sets readBatteryStatus = 0xFE so the OS WAKE ISR sees a valid battery.
    pub fn press_on_key(&mut self) {
        use crate::peripherals::interrupt::sources;

        log_evt!("ON_KEY pressed");
        // Power on the calculator
        self.powered_on = true;
        // Set the one-shot wake signal — consumed on first cpu.step() call.
        self.cpu.on_key_wake = true;

        // Set ON key in keypad matrix (row 2, col 0)
        self.bus.set_key(2, 0, true);

        // Raise INT_ON (matches CEmu: intrpt_set(INT_ON, onState) with onState truthy)
        self.bus.ports.interrupt.raise(sources::ON_KEY);

        // Handle WAKE interrupt — only when device is off (matches CEmu exactly).
        // CEmu's keypad_on_check(): if (control.off && onState) { control.off=false; intrpt_pulse(INT_WAKE); }
        // wake() clears off and sets readBatteryStatus=0xFE so the OS ISR sees valid battery.
        if self.bus.ports.control.is_off() {
            log_evt!("WAKE: device off, clearing off + pulsing WAKE");
            self.bus.ports.control.wake();
            self.bus.ports.interrupt.pulse(sources::WAKE);
            // Disable APD on every wake — if the OS put the device to sleep via APD,
            // the APD timer is still expired. Without clearing apdAble, the OS will
            // immediately re-sleep after the WAKE ISR runs, causing the screen to
            // flash briefly and become unresponsive.
            self.disable_apd();
        }

        // Ensure CPU sees a pending interrupt even if interrupts are disabled.
        // ON key wake is special: ROM expects an interrupt path to run after wake.
        self.cpu.irq_pending = true;
    }

    /// Release the ON key
    /// Clears ON_KEY raw (matches CEmu: intrpt_set(INT_ON, false) on release).
    /// WAKE is NOT touched on release — CEmu only pulses WAKE on press when off.
    /// on_key_wake is one-shot (consumed in step()), no need to clear here.
    pub fn release_on_key(&mut self) {
        use crate::peripherals::interrupt::sources;
        log_evt!("ON_KEY released");
        self.bus.set_key(2, 0, false);
        self.bus.ports.interrupt.clear_raw(sources::ON_KEY);
    }

    /// Simulate initial power-on sequence
    /// Call this after loading ROM but before run_cycles to simulate
    /// the calculator being turned on via the ON key
    pub fn power_on(&mut self) {
        // Simulate the ON key being pressed and released.
        self.press_on_key();
        self.release_on_key();
    }

    /// Get current keypad mode (for debugging)
    pub fn keypad_mode(&self) -> u8 {
        self.bus.ports.keypad.mode()
    }

    /// Render the current VRAM contents to the framebuffer
    /// Converts RGB565 to ARGB8888
    pub fn render_frame(&mut self) {
        let upbase = self.bus.ports.lcd.upbase();

        // VRAM lives in the RAM region — read directly from the backing store
        // instead of 153,600 peek_byte calls through the bus decode path.
        let ram_offset = upbase.wrapping_sub(crate::memory::addr::RAM_START) as usize;
        let needed = SCREEN_WIDTH * SCREEN_HEIGHT * 2;
        let ram_data = self.bus.ram.data();

        if ram_offset < ram_data.len() && ram_offset + needed <= ram_data.len() {
            let vram = &ram_data[ram_offset..ram_offset + needed];
            for (i, chunk) in vram.chunks_exact(2).enumerate() {
                let rgb565 = u16::from_le_bytes([chunk[0], chunk[1]]);
                let r = ((rgb565 >> 11) & 0x1F) as u8;
                let g = ((rgb565 >> 5) & 0x3F) as u8;
                let b = (rgb565 & 0x1F) as u8;
                let r8 = (r << 3) | (r >> 2);
                let g8 = (g << 2) | (g >> 4);
                let b8 = (b << 3) | (b >> 2);
                self.framebuffer[i] = 0xFF000000 | ((r8 as u32) << 16) | ((g8 as u32) << 8) | (b8 as u32);
            }
        } else {
            // Fallback for out-of-range UPBASE (e.g. before LCD is configured)
            for y in 0..SCREEN_HEIGHT {
                for x in 0..SCREEN_WIDTH {
                    let pixel_offset = (y * SCREEN_WIDTH + x) * 2;
                    let vram_addr = upbase + pixel_offset as u32;
                    let lo = self.bus.peek_byte(vram_addr) as u16;
                    let hi = self.bus.peek_byte(vram_addr + 1) as u16;
                    let rgb565 = lo | (hi << 8);
                    let r = ((rgb565 >> 11) & 0x1F) as u8;
                    let g = ((rgb565 >> 5) & 0x3F) as u8;
                    let b = (rgb565 & 0x1F) as u8;
                    let r8 = (r << 3) | (r >> 2);
                    let g8 = (g << 2) | (g >> 4);
                    let b8 = (b << 3) | (b >> 2);
                    self.framebuffer[y * SCREEN_WIDTH + x] = 0xFF000000 | ((r8 as u32) << 16) | ((g8 as u32) << 8) | (b8 as u32);
                }
            }
        }
    }

    // ========== State Persistence ==========

    /// State format version (v7: DMA scheduling, scheduler grew from 88→96 bytes for DMA state)
    const STATE_VERSION: u32 = 8;
    /// Magic bytes for state file identification
    const STATE_MAGIC: [u8; 4] = *b"CE84";
    /// Header size: magic(4) + version(4) + rom_hash(8) + data_len(4) = 20
    const STATE_HEADER_SIZE: usize = 20;
    /// Metadata size: powered_on(1) + total_cycles(8) + boot_init_done(1) + padding(6) = 16
    const STATE_META_SIZE: usize = 16;

    /// Compute a simple hash of the ROM for state validation
    fn compute_rom_hash(&self) -> u64 {
        // FNV-1a hash of first 64KB of ROM (fast, good distribution)
        let mut hash: u64 = 0xcbf29ce484222325;
        let rom_data = self.bus.flash.data();
        let len = rom_data.len().min(65536);
        for &byte in &rom_data[..len] {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }

    /// Get size required for save state buffer
    pub fn save_state_size(&self) -> usize {
        use crate::cpu::Cpu;
        use crate::memory::addr::{FLASH_SIZE, RAM_SIZE};
        use crate::peripherals::Peripherals;
        use crate::scheduler::Scheduler;

        Self::STATE_HEADER_SIZE
            + Cpu::SNAPSHOT_SIZE
            + Scheduler::SNAPSHOT_SIZE
            + Peripherals::SNAPSHOT_SIZE
            + Self::STATE_META_SIZE
            + RAM_SIZE
            + FLASH_SIZE
    }

    /// Save emulator state to buffer
    /// Returns number of bytes written on success
    pub fn save_state(&self, buffer: &mut [u8]) -> Result<usize, i32> {
        use crate::cpu::Cpu;
        use crate::memory::addr::{FLASH_SIZE, RAM_SIZE};
        use crate::peripherals::Peripherals;
        use crate::scheduler::Scheduler;

        let required = self.save_state_size();
        if buffer.len() < required {
            return Err(-101); // Buffer too small
        }

        let mut pos = 0;

        // Write header
        buffer[pos..pos+4].copy_from_slice(&Self::STATE_MAGIC);
        pos += 4;
        buffer[pos..pos+4].copy_from_slice(&Self::STATE_VERSION.to_le_bytes());
        pos += 4;
        buffer[pos..pos+8].copy_from_slice(&self.compute_rom_hash().to_le_bytes());
        pos += 8;
        let data_len = (required - Self::STATE_HEADER_SIZE) as u32;
        buffer[pos..pos+4].copy_from_slice(&data_len.to_le_bytes());
        pos += 4;

        // Write CPU state
        let cpu_bytes = self.cpu.to_bytes();
        buffer[pos..pos+Cpu::SNAPSHOT_SIZE].copy_from_slice(&cpu_bytes);
        pos += Cpu::SNAPSHOT_SIZE;

        // Write scheduler state
        let sched_bytes = self.scheduler.to_bytes();
        buffer[pos..pos+Scheduler::SNAPSHOT_SIZE].copy_from_slice(&sched_bytes);
        pos += Scheduler::SNAPSHOT_SIZE;

        // Write peripheral state
        let periph_bytes = self.bus.ports.to_bytes();
        buffer[pos..pos+Peripherals::SNAPSHOT_SIZE].copy_from_slice(&periph_bytes);
        pos += Peripherals::SNAPSHOT_SIZE;

        // Write Emu metadata
        buffer[pos] = if self.powered_on { 1 } else { 0 }; pos += 1;
        buffer[pos..pos+8].copy_from_slice(&self.total_cycles.to_le_bytes()); pos += 8;
        buffer[pos] = if self.boot_init_done { 1 } else { 0 }; pos += 1;
        pos += 6; // Padding to 16 bytes

        // Write RAM
        let ram_data = self.bus.ram.data();
        buffer[pos..pos+RAM_SIZE].copy_from_slice(ram_data);
        pos += RAM_SIZE;

        // Write Flash
        let flash_data = self.bus.flash.data();
        buffer[pos..pos+FLASH_SIZE].copy_from_slice(flash_data);
        pos += FLASH_SIZE;

        log_evt!("STATE_SAVED: {} bytes", pos);
        Ok(pos)
    }

    /// Load emulator state from buffer
    pub fn load_state(&mut self, buffer: &[u8]) -> Result<(), i32> {
        use crate::cpu::Cpu;
        use crate::memory::addr::{FLASH_SIZE, RAM_SIZE};
        use crate::peripherals::Peripherals;
        use crate::scheduler::Scheduler;

        // Check minimum size for header
        if buffer.len() < Self::STATE_HEADER_SIZE {
            return Err(-102); // Invalid magic / too small
        }

        let mut pos = 0;

        // Verify magic
        if &buffer[pos..pos+4] != &Self::STATE_MAGIC {
            return Err(-102); // Invalid magic
        }
        pos += 4;

        // Check version
        let version = u32::from_le_bytes(buffer[pos..pos+4].try_into().unwrap());
        if version != Self::STATE_VERSION {
            return Err(-103); // Version mismatch
        }
        pos += 4;

        // Verify ROM hash
        let saved_hash = u64::from_le_bytes(buffer[pos..pos+8].try_into().unwrap());
        let current_hash = self.compute_rom_hash();
        if saved_hash != current_hash {
            return Err(-104); // ROM mismatch
        }
        pos += 8;

        // Check data length
        let data_len = u32::from_le_bytes(buffer[pos..pos+4].try_into().unwrap()) as usize;
        pos += 4;

        let expected_data = Cpu::SNAPSHOT_SIZE + Scheduler::SNAPSHOT_SIZE
            + Peripherals::SNAPSHOT_SIZE + Self::STATE_META_SIZE + RAM_SIZE + FLASH_SIZE;
        if data_len < expected_data || buffer.len() < pos + data_len {
            return Err(-105); // Data corruption
        }

        // Load CPU state
        self.cpu.from_bytes(&buffer[pos..pos+Cpu::SNAPSHOT_SIZE])?;
        pos += Cpu::SNAPSHOT_SIZE;

        // Load scheduler state
        self.scheduler.from_bytes(&buffer[pos..pos+Scheduler::SNAPSHOT_SIZE])?;
        pos += Scheduler::SNAPSHOT_SIZE;

        // Load peripheral state
        self.bus.ports.from_bytes(&buffer[pos..pos+Peripherals::SNAPSHOT_SIZE])?;
        pos += Peripherals::SNAPSHOT_SIZE;

        // Load Emu metadata
        self.powered_on = buffer[pos] != 0; pos += 1;
        self.total_cycles = u64::from_le_bytes(buffer[pos..pos+8].try_into().unwrap()); pos += 8;
        self.boot_init_done = buffer[pos] != 0; pos += 1;
        pos += 6; // Skip padding

        // Load RAM
        self.bus.ram.load_data(&buffer[pos..pos+RAM_SIZE]);
        pos += RAM_SIZE;

        // Load Flash
        self.bus.flash.load_data(&buffer[pos..pos+FLASH_SIZE]);

        // Sync bus cycle counter with restored total_cycles.
        // load_rom() → reset() zeroed bus.cycles, but total_cycles was restored
        // from metadata. Without this sync, the first self.total_cycles =
        // self.bus.total_cycles() in run_cycles() would overwrite the restored
        // value, causing the `executed` return to wrap.
        self.bus.set_total_cycles(self.total_cycles);

        // Clear stale cpu_speed_written flag left by from_bytes calling
        // control.write(0x01, ...).  Without this, the next control-port
        // write would trigger a spurious cycle conversion on already-correct
        // scheduler timestamps.
        self.bus.ports.control.cpu_speed_changed();

        // Reset transient state
        self.rom_loaded = true;
        self.halt_logged = false;
        self.history.clear();
        self.last_stop = StopReason::CyclesComplete;

        log_evt!(
            "STATE_LOADED total_cycles={} bus_cycles={} base_ticks={} dma_ts={} cpu_speed={} pc={:06X}",
            self.total_cycles,
            self.bus.total_cycles(),
            self.scheduler.base_ticks,
            self.scheduler.dma_last_mem_timestamp,
            self.scheduler.cpu_speed(),
            self.cpu.pc
        );
        Ok(())
    }

    /// Get the last stop reason
    pub fn last_stop_reason(&self) -> StopReason {
        self.last_stop
    }

    /// Get current PC
    pub fn pc(&self) -> u32 {
        self.cpu.pc
    }

    /// Get C register value
    pub fn c_register(&self) -> u8 {
        self.cpu.c()
    }

    /// Get A register value
    pub fn a_register(&self) -> u8 {
        self.cpu.a
    }

    /// Get DE register value
    pub fn de_register(&self) -> u32 {
        self.cpu.de
    }

    /// Get B register value
    pub fn b_register(&self) -> u8 {
        self.cpu.b()
    }

    /// Get BC register value
    pub fn bc_register(&self) -> u32 {
        self.cpu.bc
    }

    /// Get HL register value
    pub fn hl_register(&self) -> u32 {
        self.cpu.hl
    }

    /// Get IY register value
    pub fn iy_register(&self) -> u32 {
        self.cpu.iy
    }

    /// Get IX register value
    pub fn ix_register(&self) -> u32 {
        self.cpu.ix
    }

    /// Get F (flags) register value
    pub fn f_register(&self) -> u8 {
        self.cpu.f
    }

    /// Check if CPU is halted
    pub fn is_halted(&self) -> bool {
        self.cpu.halted
    }

    /// Get total cycles executed
    pub fn total_cycles(&self) -> u64 {
        self.total_cycles
    }

    /// Get raw bus cycle counter (resets on CPU speed change like CEmu)
    /// This returns total cycles (CPU + memory timing).
    pub fn bus_cycles(&self) -> u64 {
        self.bus.total_cycles()
    }

    /// Get CPU-only cycle counter (matches CEmu's cpu.cycles for trace comparison)
    /// This excludes memory timing - only counts internal CPU events like:
    /// - Branch taken cycles
    /// - HALT cycles
    /// - (HL) operand cycles
    /// - Block instruction internal cycles
    pub fn cpu_cycles(&self) -> u64 {
        self.bus.cycles()
    }

    /// Peek at a memory byte without affecting emulation state
    pub fn peek_byte(&mut self, addr: u32) -> u8 {
        self.bus.peek_byte(addr)
    }

    /// Poke a memory byte (for debugging/testing)
    pub fn poke_byte(&mut self, addr: u32, value: u8) {
        self.bus.write_byte(addr, value);
    }

    /// High-level key injection (like CEmu's sendKey)
    /// This writes directly to TI-OS memory locations, bypassing hardware keypad.
    /// Returns true if key was successfully injected, false if TI-OS wasn't ready.
    ///
    /// TI-OS key addresses:
    /// - CE_kbdKey (0xD0058C) = key code high byte
    /// - CE_keyExtend (0xD0058E) = key code low byte
    /// - CE_graphFlags2 (0xD0009F) bit 5 = keyReady flag
    ///
    /// Key codes (from CEmu):
    /// - ENTER = 0x05
    /// - CLEAR = 0x09
    /// - Numbers: '0' = 0x8E, '1' = 0x8F, ... '9' = 0x97
    pub fn send_key(&mut self, key: u16) -> bool {
        const CE_KBD_KEY: u32 = 0xD0058C;
        const CE_KEY_EXTEND: u32 = 0xD0058E;
        const CE_GRAPH_FLAGS2: u32 = 0xD0009F;
        const CE_KEY_READY: u8 = 1 << 5;

        let flags = self.peek_byte(CE_GRAPH_FLAGS2);
        if (flags & CE_KEY_READY) != 0 {
            // TI-OS hasn't processed previous key yet
            return false;
        }

        // If key < 0x100, shift to high byte (CEmu convention)
        let key = if key < 0x100 { key << 8 } else { key };

        self.poke_byte(CE_KBD_KEY, (key >> 8) as u8);
        self.poke_byte(CE_KEY_EXTEND, (key & 0xFF) as u8);
        self.poke_byte(CE_GRAPH_FLAGS2, flags | CE_KEY_READY);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let verify_key = self.peek_byte(CE_KBD_KEY);
            let verify_extend = self.peek_byte(CE_KEY_EXTEND);
            let verify_flags = self.peek_byte(CE_GRAPH_FLAGS2);
            log_evt!("SEND_KEY: key=0x{:04X} wrote kbdKey=0x{:02X} keyExtend=0x{:02X} flags=0x{:02X}",
                key, verify_key, verify_extend, verify_flags);
        }
        true
    }

    /// High-level key injection for letter/number keys
    /// '0'-'9' -> 0x8E-0x97
    /// 'A'-'Z' -> 0x9A-0xB3
    pub fn send_letter_key(&mut self, letter: char) -> bool {
        let key = match letter {
            '0'..='9' => 0x8E + (letter as u16 - '0' as u16),
            'A'..='Z' => 0x9A + (letter as u16 - 'A' as u16),
            _ => return false,
        };
        self.send_key(key)
    }

    /// Get the CPU's A register value
    pub fn reg_a(&self) -> u8 {
        self.cpu.a
    }

    /// Alias for reg_a
    pub fn a(&self) -> u8 {
        self.cpu.a
    }

    /// Get the CPU's F register value (flags)
    pub fn reg_f(&self) -> u8 {
        self.cpu.f
    }

    /// Alias for reg_f
    pub fn f(&self) -> u8 {
        self.cpu.f
    }

    /// Get the CPU's stack pointer
    pub fn sp(&self) -> u32 {
        self.cpu.sp()
    }

    /// Get the CPU's BC register
    pub fn bc(&self) -> u32 {
        self.cpu.bc
    }

    /// Get the CPU's DE register
    pub fn de(&self) -> u32 {
        self.cpu.de
    }

    /// Get the CPU's HL register
    pub fn hl(&self) -> u32 {
        self.cpu.hl
    }

    /// Get the CPU's IX register
    pub fn ix(&self) -> u32 {
        self.cpu.ix
    }

    /// Get the CPU's IY register
    pub fn iy(&self) -> u32 {
        self.cpu.iy
    }

    /// Get the CPU's I register (interrupt vector base)
    pub fn reg_i(&self) -> u16 {
        self.cpu.i
    }

    /// Get IFF1 (interrupt enable flag)
    pub fn iff1(&self) -> bool {
        self.cpu.iff1
    }

    /// Get IFF2 (interrupt enable shadow)
    pub fn iff2(&self) -> bool {
        self.cpu.iff2
    }

    /// Get interrupt mode
    pub fn interrupt_mode(&self) -> InterruptMode {
        self.cpu.im
    }

    /// Get ADL mode flag
    pub fn adl(&self) -> bool {
        self.cpu.adl
    }

    /// Get IRQ pending flag
    pub fn irq_pending(&self) -> bool {
        self.cpu.irq_pending
    }

    /// Get NMI pending flag
    pub fn nmi_pending(&self) -> bool {
        self.cpu.nmi_pending
    }

    /// Get ON-key wake flag
    pub fn on_key_wake(&self) -> bool {
        self.cpu.on_key_wake
    }

    /// Get any-key wake flag
    pub fn any_key_wake(&self) -> bool {
        self.cpu.any_key_wake
    }

    /// Read full interrupt status mask
    pub fn interrupt_status(&self) -> u32 {
        let lo = self.bus.ports.interrupt.read(0x00) as u32;
        let b1 = (self.bus.ports.interrupt.read(0x01) as u32) << 8;
        let b2 = (self.bus.ports.interrupt.read(0x02) as u32) << 16;
        let b3 = (self.bus.ports.interrupt.read(0x03) as u32) << 24;
        lo | b1 | b2 | b3
    }

    /// Read full interrupt enabled mask
    pub fn interrupt_enabled(&self) -> u32 {
        let lo = self.bus.ports.interrupt.read(0x04) as u32;
        let b1 = (self.bus.ports.interrupt.read(0x05) as u32) << 8;
        let b2 = (self.bus.ports.interrupt.read(0x06) as u32) << 16;
        let b3 = (self.bus.ports.interrupt.read(0x07) as u32) << 24;
        lo | b1 | b2 | b3
    }

    /// Read full interrupt raw mask
    pub fn interrupt_raw(&self) -> u32 {
        let lo = self.bus.ports.interrupt.read(0x08) as u32;
        let b1 = (self.bus.ports.interrupt.read(0x09) as u32) << 8;
        let b2 = (self.bus.ports.interrupt.read(0x0A) as u32) << 16;
        let b3 = (self.bus.ports.interrupt.read(0x0B) as u32) << 24;
        lo | b1 | b2 | b3
    }

    /// Read a control port byte (offset from 0xE00000)
    pub fn control_read(&self, offset: u32) -> u8 {
        self.bus.ports.control.read(offset)
    }

    /// Mask an instruction address based on ADL/MBASE (debug helper)
    pub fn mask_addr(&self, addr: u32) -> u32 {
        self.cpu.mask_addr_instr(addr)
    }

    /// Snapshot a timer's internal state (1, 2, or 3)
    pub fn timer_snapshot(&self, which: usize) -> Option<TimerSnapshot> {
        let idx = match which {
            1 => 0,
            2 => 1,
            3 => 2,
            _ => return None,
        };
        let timers = &self.bus.ports.timers;

        Some(TimerSnapshot {
            counter: timers.counter(idx),
            reset_value: timers.reset_value(idx),
            match1: timers.match_val(idx, 0),
            match2: timers.match_val(idx, 1),
            control: ((timers.control_word() >> (idx * 3)) & 0x7) as u8
                | if timers.control_word() & (1 << (9 + idx)) != 0 { 0x08 } else { 0 },
        })
    }

    /// Snapshot LCD controller state
    pub fn lcd_snapshot(&self) -> LcdSnapshot {
        let lcd = &self.bus.ports.lcd;
        LcdSnapshot {
            timing: lcd.timing(),
            control: lcd.control(),
            int_mask: lcd.int_mask(),
            int_status: lcd.int_status(),
            upbase: lcd.upbase(),
            lpbase: lcd.lpbase(),
            compare_state: lcd.compare_state(),
        }
    }

    /// Debug: Get flash unlock status for diagnostics
    pub fn debug_flash_status(&self) -> String {
        let ctrl = &self.bus.ports.control;
        format!(
            "Flash unlock: port0x06={:02X} (protected_unlocked={}), port0x28={:02X} (flash_ready={}), privileged=0x{:06X}",
            ctrl.read(0x06),
            ctrl.protected_ports_unlocked(),
            ctrl.read(0x28),
            ctrl.flash_ready(),
            ctrl.privileged_boundary()
        )
    }

    /// Dump control port values for comparison with CEmu
    pub fn dump_control_ports(&self) -> String {
        self.bus.ports.control.dump()
    }

    /// Dump execution history for debugging
    /// Returns a string with the last N instructions executed
    pub fn dump_history(&self) -> String {
        let mut output = String::new();
        output.push_str("Execution history (oldest to newest):\n");

        for entry in self.history.iter() {
            let opcode_str: String = entry.opcode[..entry.opcode_len as usize]
                .iter()
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<_>>()
                .join(" ");

            output.push_str(&format!(
                "  PC={:06X}  {:12}  {}\n",
                entry.pc,
                opcode_str,
                Self::disassemble_opcode(&entry.opcode[..entry.opcode_len as usize])
            ));
        }

        output.push_str(&format!("\nCurrent PC: {:06X}\n", self.cpu.pc));
        output.push_str(&format!("Total cycles: {}\n", self.total_cycles));
        output.push_str(&format!("Stop reason: {:?}\n", self.last_stop));

        output
    }

    /// Simple disassembler for common opcodes
    fn disassemble_opcode(opcode: &[u8]) -> &'static str {
        if opcode.is_empty() {
            return "???";
        }

        match opcode[0] {
            0x00 => "NOP",
            0x01 => "LD BC,nn",
            0x02 => "LD (BC),A",
            0x03 => "INC BC",
            0x04 => "INC B",
            0x05 => "DEC B",
            0x06 => "LD B,n",
            0x07 => "RLCA",
            0x08 => "EX AF,AF'",
            0x09 => "ADD HL,BC",
            0x0A => "LD A,(BC)",
            0x0B => "DEC BC",
            0x0C => "INC C",
            0x0D => "DEC C",
            0x0E => "LD C,n",
            0x0F => "RRCA",
            0x10 => "DJNZ d",
            0x11 => "LD DE,nn",
            0x12 => "LD (DE),A",
            0x18 => "JR d",
            0x20 => "JR NZ,d",
            0x21 => "LD HL,nn",
            0x22 => "LD (nn),HL",
            0x23 => "INC HL",
            0x28 => "JR Z,d",
            0x2A => "LD HL,(nn)",
            0x30 => "JR NC,d",
            0x31 => "LD SP,nn",
            0x32 => "LD (nn),A",
            0x38 => "JR C,d",
            0x3A => "LD A,(nn)",
            0x3E => "LD A,n",
            0x76 => "HALT",
            0xC0 => "RET NZ",
            0xC1 => "POP BC",
            0xC2 => "JP NZ,nn",
            0xC3 => "JP nn",
            0xC4 => "CALL NZ,nn",
            0xC5 => "PUSH BC",
            0xC6 => "ADD A,n",
            0xC7 => "RST 00H",
            0xC8 => "RET Z",
            0xC9 => "RET",
            0xCA => "JP Z,nn",
            0xCB => "CB prefix",
            0xCD => "CALL nn",
            0xD0 => "RET NC",
            0xD1 => "POP DE",
            0xD5 => "PUSH DE",
            0xD8 => "RET C",
            0xD9 => "EXX",
            0xDD => "DD prefix (IX)",
            0xE1 => "POP HL",
            0xE5 => "PUSH HL",
            0xE9 => "JP (HL)",
            0xEB => "EX DE,HL",
            0xED => "ED prefix",
            0xF1 => "POP AF",
            0xF3 => "DI",
            0xF5 => "PUSH AF",
            0xFB => "EI",
            0xFD => "FD prefix (IY)",
            0xFE => "CP n",
            0xFF => "RST 38H",
            _ => "...",
        }
    }

    /// Enable RAM write tracing for debugging
    pub fn enable_write_tracing(&mut self) {
        self.bus.write_tracer.enable();
    }

    /// Disable RAM write tracing
    pub fn disable_write_tracing(&mut self) {
        self.bus.write_tracer.disable();
    }

    /// Reset write trace data (keeps enabled state)
    pub fn reset_write_trace(&mut self) {
        self.bus.write_tracer.reset();
    }

    /// Get write trace summary
    pub fn write_trace_summary(&self) -> String {
        self.bus.write_tracer.summary()
    }

    /// Get total number of RAM writes traced
    pub fn write_trace_total(&self) -> u64 {
        self.bus.write_tracer.total_writes()
    }

    /// Get number of unique RAM addresses written
    pub fn write_trace_unique_addresses(&self) -> usize {
        self.bus.write_tracer.unique_addresses()
    }

    /// Check if a specific address was written during tracing
    pub fn was_address_written(&self, addr: u32) -> bool {
        self.bus.write_tracer.was_written(addr)
    }

    /// Get write count for a specific address
    pub fn address_write_count(&self, addr: u32) -> u32 {
        self.bus.write_tracer.write_count(addr)
    }

    /// Set a filter to only trace writes within an address range
    pub fn set_write_trace_filter(&mut self, start: u32, end: u32) {
        self.bus.write_tracer.set_filter_range(start, end);
    }

    /// Clear write trace filter (trace all RAM writes)
    pub fn clear_write_trace_filter(&mut self) {
        self.bus.write_tracer.clear_filter_range();
    }

    /// Get detailed write log (Vec of (addr, value, cycle))
    pub fn get_write_log(&self) -> Vec<(u32, u8, u64)> {
        self.bus.write_tracer.detailed_log()
            .iter()
            .map(|rec| (rec.addr, rec.value, rec.cycle))
            .collect()
    }

    /// Get CPU register dump for debugging
    pub fn dump_registers(&self) -> String {
        format!(
            "AF={:02X}{:02X} BC={:06X} DE={:06X} HL={:06X}\n\
             IX={:06X} IY={:06X} SP={:06X} PC={:06X}\n\
             Flags: S={} Z={} H={} PV={} N={} C={}\n\
             ADL={} IFF1={} IFF2={} IM={:?} MBASE={:02X}",
            self.cpu.a,
            self.cpu.f,
            self.cpu.bc,
            self.cpu.de,
            self.cpu.hl,
            self.cpu.ix,
            self.cpu.iy,
            self.cpu.sp(),
            self.cpu.pc,
            (self.cpu.f >> 7) & 1,
            (self.cpu.f >> 6) & 1,
            (self.cpu.f >> 4) & 1,
            (self.cpu.f >> 2) & 1,
            (self.cpu.f >> 1) & 1,
            self.cpu.f & 1,
            self.cpu.adl,
            self.cpu.iff1,
            self.cpu.iff2,
            self.cpu.im,
            self.cpu.mbase,
        )
    }

    // === Full I/O Trace Methods ===

    /// Enable full I/O tracing (records all memory/port operations per instruction)
    pub fn enable_full_trace(&mut self) {
        self.bus.enable_full_trace();
    }

    /// Disable full I/O tracing
    pub fn disable_full_trace(&mut self) {
        self.bus.disable_full_trace();
    }

    /// Check if full I/O tracing is enabled
    pub fn is_full_trace_enabled(&self) -> bool {
        self.bus.is_full_trace_enabled()
    }
}

impl Default for Emu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_emu() {
        let emu = Emu::new();
        assert_eq!(emu.framebuffer.len(), SCREEN_WIDTH * SCREEN_HEIGHT);
        assert!(!emu.rom_loaded);
    }

    #[test]
    fn test_load_rom() {
        let mut emu = Emu::new();
        // Minimal ROM - flash defaults to 0xFF so we only need the bytes we use
        let rom = vec![0x00, 0x00, 0x76]; // NOP, NOP, HALT
        assert!(emu.load_rom(&rom).is_ok());
        assert!(emu.rom_loaded);
    }

    #[test]
    fn test_empty_rom_fails() {
        let mut emu = Emu::new();
        let rom: Vec<u8> = vec![];
        assert!(emu.load_rom(&rom).is_err());
    }

    #[test]
    fn test_key_state() {
        let mut emu = Emu::new();
        emu.set_key(0, 0, true);
        assert!(emu.bus.key_state()[0][0]);
        emu.set_key(0, 0, false);
        assert!(!emu.bus.key_state()[0][0]);
    }

    #[test]
    fn test_run_cycles() {
        let mut emu = Emu::new();
        // Without ROM loaded, should return 0
        let executed = emu.run_cycles(1000);
        assert_eq!(executed, 0);
    }

    #[test]
    fn test_run_with_rom() {
        let mut emu = Emu::new();
        // Minimal ROM - flash defaults to 0xFF so we only need the bytes we use
        let rom = vec![0x00, 0x00, 0x00, 0x76]; // NOP, NOP, NOP, HALT
        emu.load_rom(&rom).unwrap();
        emu.powered_on = true; // Power on for test
        let executed = emu.run_cycles(1000);

        // Should have executed some cycles and halted
        // Note: Since we don't return early on HALT (to keep peripherals ticking),
        // the stop reason is CyclesComplete, but the CPU IS halted.
        assert!(executed > 0);
        assert_eq!(emu.last_stop_reason(), StopReason::CyclesComplete);
        assert!(emu.cpu.halted);
    }

    #[test]
    fn test_reset() {
        let mut emu = Emu::new();
        // Minimal ROM - flash defaults to 0xFF so we only need the bytes we use
        let rom = vec![0x00, 0x76]; // NOP, HALT
        emu.load_rom(&rom).unwrap();
        emu.powered_on = true; // Power on for test
        emu.run_cycles(100);
        emu.set_key(1, 1, true);
        emu.reset();

        assert_eq!(emu.cpu.pc, 0);
        assert!(!emu.bus.key_state()[1][1]);
        // After reset, total_cycles includes init_prefetch cost for CEmu parity.
        // The prefetch fetches the first byte at PC=0, which adds flash timing cycles.
        // With serial flash enabled by default, this is typically 10 cycles (cache miss).
        // We just verify it's small and consistent, rather than exactly 0.
        assert!(emu.total_cycles <= 20, "total_cycles after reset should be small prefetch cost");
        assert!(!emu.powered_on); // Reset should power off the calculator
    }

    #[test]
    fn test_history() {
        let mut emu = Emu::new();
        // Minimal ROM - flash defaults to 0xFF so we only need the bytes we use
        let rom = vec![0x00, 0x00, 0x00, 0x76]; // NOP, NOP, NOP, HALT
        emu.load_rom(&rom).unwrap();
        emu.powered_on = true; // Power on for test
        emu.run_cycles(100);

        let history = emu.dump_history();
        assert!(history.contains("NOP"));
        assert!(history.contains("HALT"));
    }

    #[test]
    fn test_on_key_wakes_from_halt_with_di() {
        let mut emu = Emu::new();
        // ROM: DI (F3), HALT (76), NOP (00), NOP (00)
        // After DI + HALT, interrupts are disabled but ON key should still wake
        let rom = vec![0xF3, 0x76, 0x00, 0x00];
        emu.load_rom(&rom).unwrap();
        emu.powered_on = true; // Power on for test (without ON key side effects)

        // Run until HALT
        emu.run_cycles(100);
        assert!(emu.cpu.halted);
        assert!(!emu.cpu.iff1); // Interrupts are disabled

        // Press ON key - should wake CPU even though interrupts are disabled
        emu.press_on_key();
        assert!(emu.cpu.on_key_wake); // One-shot signal set

        // Run some more cycles - CPU should wake and execute NOPs
        let cycles_before = emu.total_cycles;
        emu.run_cycles(20);

        // Verify CPU woke up and executed instructions
        assert!(!emu.cpu.halted);
        assert!(emu.total_cycles > cycles_before);
        assert!(emu.cpu.pc > 2); // PC moved past HALT
        assert!(!emu.cpu.on_key_wake); // One-shot consumed by step()
    }

    #[test]
    fn test_on_key_wake_sets_iff_for_pending_irq() {
        let mut emu = Emu::new();
        // ROM: DI (F3), HALT (76), NOP (00)
        let rom = vec![0xF3, 0x76, 0x00];
        emu.load_rom(&rom).unwrap();
        emu.powered_on = true; // Power on for test (without ON key side effects)

        // Run until HALT with interrupts disabled
        emu.run_cycles(100);
        assert!(emu.cpu.halted);
        assert!(!emu.cpu.iff1);

        // Press ON key to wake
        emu.press_on_key();
        assert!(emu.cpu.on_key_wake); // One-shot set

        // Manually step once to process the wake path
        // on_key_wake path: consumes one-shot, clears halted, enables IFF, adds 1 cycle
        let cycles = emu.cpu.step(&mut emu.bus);
        assert_eq!(cycles, 1);
        assert!(!emu.cpu.halted);
        assert!(emu.cpu.iff1);
        assert!(emu.cpu.irq_pending);
        assert!(!emu.cpu.on_key_wake); // One-shot consumed
    }

    #[test]
    fn test_on_key_raises_interrupt() {
        use crate::peripherals::interrupt::sources;

        let mut emu = Emu::new();
        let rom = vec![0x00]; // NOP
        emu.load_rom(&rom).unwrap();

        // Press ON key
        emu.press_on_key();

        // ON_KEY interrupt should be raised in status
        let status = emu.bus.ports.interrupt.read(0x00);
        assert_eq!(status & (sources::ON_KEY as u8), sources::ON_KEY as u8);
    }

    #[test]
    fn test_on_key_interrupt_behavior() {
        use crate::peripherals::interrupt::sources;

        let mut emu = Emu::new();
        let rom = vec![0x00; 16]; // NOPs
        emu.load_rom(&rom).unwrap();

        // Press ON key — should set ON_KEY in interrupt status.
        // WAKE is NOT raised when device is not off (matches CEmu).
        emu.press_on_key();

        let status = emu.bus.ports.interrupt.read(0x00);
        assert_ne!(status & (sources::ON_KEY as u8), 0,
            "ON_KEY status should be set after press");

        // Release ON key — ON_KEY raw clears (matches CEmu: intrpt_set(INT_ON, false))
        emu.release_on_key();

        // ON_KEY should clear after release (non-latched follows raw)
        let status_after = emu.bus.ports.interrupt.read(0x00);
        assert_eq!(status_after & (sources::ON_KEY as u8), 0,
            "ON_KEY status should clear after release (non-latched)");
    }

    #[test]
    fn test_on_key_wake_from_off() {
        use crate::peripherals::interrupt::sources;

        let mut emu = Emu::new();
        let rom = vec![0x00; 16]; // NOPs
        emu.load_rom(&rom).unwrap();

        // Simulate device being off (OS wrote bit 6 to port 0x00)
        emu.bus.ports.control.write(0x00, 0x40); // Set bit 6 → off=true
        assert!(emu.bus.ports.control.is_off());

        // Configure WAKE as inverted (matches what the OS does before sleeping)
        // Inverted register is at offset 0x10 in interrupt controller
        // WAKE is bit 19 → byte 2 (bits 16-23), bit 3
        emu.bus.ports.interrupt.write(0x12, (sources::WAKE >> 16) as u8);

        // Press ON key — should pulse WAKE and clear off
        emu.press_on_key();

        assert!(!emu.bus.ports.control.is_off(), "off should be cleared after wake");
        // For inverted WAKE, pulse sets status via the clear step
        let wake_byte = emu.bus.ports.interrupt.read(0x02); // status bits 16-23
        assert_ne!(wake_byte & ((sources::WAKE >> 16) as u8), 0,
            "WAKE status should be set after pulse (inverted logic)");
    }

    #[test]
    fn test_regular_interrupt_cannot_wake_with_di() {
        let mut emu = Emu::new();
        // ROM: DI (F3), HALT (76), NOP (00)
        let rom = vec![0xF3, 0x76, 0x00];
        emu.load_rom(&rom).unwrap();
        emu.powered_on = true; // Power on for test (without triggering ON key wake)

        // Run until HALT
        emu.run_cycles(100);
        assert!(emu.cpu.halted);
        assert!(!emu.cpu.iff1);

        // Set regular IRQ pending - should NOT wake because IFF1 is false
        emu.cpu.irq_pending = true;

        emu.run_cycles(20);

        // CPU should still be halted (regular IRQ can't wake with DI)
        assert!(emu.cpu.halted);
    }
}
