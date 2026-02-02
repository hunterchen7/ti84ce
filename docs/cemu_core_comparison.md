# CEmu Core Comparison (Rust core vs cemu-ref/core)

High-level differences observed while comparing the Rust core (`core/`) to the CEmu reference core (`cemu-ref/core/`). This is not exhaustive, but it captures the major architectural and behavioral gaps that can affect parity.

## Scope

- Rust core: `core/src/*`
- CEmu reference core: `cemu-ref/core/*`

## Entry Points and State

- Rust exposes a stable C ABI with a mutexed `SyncEmu`, custom save-state format, and a log callback (`core/src/lib.rs`, `core/src/emu.rs`).
- CEmu uses global state and `emu_load/emu_run/emu_save` with ASIC-driven initialization and serialized state files (`cemu-ref/core/emu.c`, `cemu-ref/core/asic.c`).
- Rust gates execution on `rom_loaded` and `powered_on`, and injects an ENTER after boot to initialize the TI-OS parser (`core/src/emu.rs`); CEmu runs after load and has no boot-screen ENTER injection (`cemu-ref/core/emu.c`).
- Rust adds instruction tracing, execution history, and RAM write tracing (`core/src/emu.rs`, `core/src/bus.rs`); CEmu relies on its debug subsystem (`cemu-ref/core/debug/*`).

## CPU Execution Model

- Prefetch/pipeline: CEmu uses an explicit prefetch pipeline (`cemu-ref/core/cpu.c`); Rust fetches directly via `bus.fetch_byte` (`core/src/cpu/helpers.rs`).
- Cycle accounting: Rust counts bus access cycles only; instruction cycle counts returned by `execute_*` are not applied (`core/src/cpu/mod.rs`, `core/src/cpu/execute.rs`). CEmu updates `cpu.cycles` per instruction and scheduler events (`cemu-ref/core/cpu.c`, `cemu-ref/core/schedule.c`).
- Protection enforcement: CEmu enforces unprivileged behavior (OUT triggers NMI, protected reads return 0, protected writes trigger NMI) (`cemu-ref/core/cpu.c`, `cemu-ref/core/mem.c`, `cemu-ref/core/control.c`). Rust tracks protected ranges but does not enforce these checks in CPU/bus paths (`core/src/peripherals/control.rs`, `core/src/bus.rs`).
- Signals: CEmu uses atomic CPU signals (RESET/EXIT/ON/ANY) (`cemu-ref/core/cpu.c`, `cemu-ref/core/emu.c`). Rust uses `on_key_wake`/`any_key_wake` flags and has no RESET/EXIT signal path (`core/src/cpu/mod.rs`, `core/src/emu.rs`).
- Interrupt modes: CEmu supports IM3 with ASIC gating; Rust maps IM3 to Mode2 and does not implement IM3 (`cemu-ref/core/cpu.c`, `cemu-ref/core/asic.h`, `core/src/cpu/execute.rs`).

## Bus, Memory, and Flash

- Flash mapping and cache: CEmu models flash mapping, wait-states, serial/parallel flash, and a cache (`cemu-ref/core/flash.c`, `cemu-ref/core/mem.c`, `cemu-ref/core/asic.h`). Rust uses a fixed 4MB flash with constant wait states and no cache or serial-flash mode (`core/src/memory.rs`, `core/src/bus.rs`, `core/src/peripherals/flash.rs`).
- Flash command set: Rust implements a minimal AMD-style command subset (`core/src/memory.rs`). CEmu implements a larger SPI flash command set and protection state (`cemu-ref/core/flash.c`, `cemu-ref/core/mem.c`).
- Unlock sequence: Rust accepts single-DI or double-DI sequences; CEmu uses the double-DI sequence (`core/src/bus.rs`, `cemu-ref/core/mem.c`).
- MMIO mapping: Rust treats all `0xE00000-0xFFFFFF` as peripheral space with fallback storage (`core/src/bus.rs`, `core/src/peripherals/mod.rs`). CEmu only maps specific ranges and treats other MMIO addresses as unmapped with different timing; LCD palette/cursor memory is mapped into the address space (`cemu-ref/core/mem.c`, `cemu-ref/core/lcd.h`).
- Unmapped reads: Rust uses an LFSR RNG (`core/src/bus.rs`); CEmu uses `bus_rand()` and region-specific cached values with different timing (`cemu-ref/core/bus.c`, `cemu-ref/core/mem.c`).
- Port I/O timing: CEmu processes pending scheduler events mid-instruction and applies a write delay with a rewind (`cemu-ref/core/port.c`). Rust applies fixed read/write cycles and does not rewind (`core/src/bus.rs`).

## Scheduler and Timing

- Rust scheduler covers a limited set of events (RTC, SPI stub, timers, LCD) and is driven explicitly by `Emu::run_cycles` (`core/src/scheduler.rs`, `core/src/emu.rs`).
- CEmu scheduler covers many more event types (keypad scan, watchdog, LCD DMA, USB, UART, panel, etc.) and is tightly integrated with CPU timing (`cemu-ref/core/schedule.c/.h`).

## Peripherals

- Interrupt controller: Rust exposes raw status on read index 2/10; CEmu returns 0 for those registers (`core/src/peripherals/interrupt.rs`, `cemu-ref/core/interrupt.c`).
- Timers: Rust uses a simplified per-timer model with immediate interrupts; CEmu uses GPT with control/status/mask/revision registers and delayed interrupt delivery (`core/src/peripherals/timer.rs`, `cemu-ref/core/timers.c`).
- OS Timer: Rust updates it directly in `tick()`; CEmu drives it via scheduled events (`core/src/peripherals/mod.rs`, `cemu-ref/core/timers.c`).
- RTC: Rust is mostly a stub (no ticking/alarms/full load/latch state machine); CEmu implements full tick/latch/load/alarm behavior (`core/src/peripherals/rtc.rs`, `cemu-ref/core/realclock.c`).
- Keypad: Rust has simplified scan logic and no ghosting/GPIO; CEmu implements ghosting, GPIO, and scheduler-driven scanning (`core/src/peripherals/keypad.rs`, `cemu-ref/core/keypad.c`).
- LCD: Rust models a small register set and a fixed 60Hz VBLANK; CEmu implements full timings, palette/cursor, DMA, and panel integration (`core/src/peripherals/lcd.rs`, `cemu-ref/core/lcd.c/.h`, `cemu-ref/core/panel.c`).
- SPI: Rust is a partial controller stub (no device transfers); CEmu implements full RX/TX FIFO behavior and device callbacks (`core/src/peripherals/spi.rs`, `cemu-ref/core/spi.c/.h`).
- SHA256: Rust is a stub; CEmu computes actual hashes (`core/src/peripherals/sha256.rs`, `cemu-ref/core/sha256.c`).
- Backlight: Rust tracks brightness only; CEmu maintains additional port state and gamma scaling (`core/src/peripherals/backlight.rs`, `cemu-ref/core/backlight.c`).
- Watchdog: Rust is a stub; CEmu has a full watchdog state machine with scheduler integration (`core/src/peripherals/watchdog.rs`, `cemu-ref/core/misc.c/.h`).

## Missing or Extra Subsystems

- Present in CEmu but not in Rust core: ASIC/device revision handling, certificate parsing, boot version, USB, UART, link, VAT/extras, panel/gamma, DMA paths, OS glue (`cemu-ref/core/asic.*`, `cert.*`, `bootver.*`, `usb/*`, `uart.*`, `link.*`, `vat.*`, `panel.*`, `extras.*`).
- Present in Rust but not in CEmu core: C ABI wrapper with `SyncEmu`, instruction tracing/history, RAM write tracing, custom save-state format (`core/src/lib.rs`, `core/src/emu.rs`, `core/src/bus.rs`).
