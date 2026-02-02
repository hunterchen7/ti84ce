/*
 * CEmu Adapter Implementation
 *
 * Wraps CEmu's global-state API to provide instance-based interface.
 * Since emu.c has conflicting function names, we implement needed functionality here.
 *
 * Performance Note:
 * Define CEMU_PERF_INSTRUMENTATION to enable timing instrumentation.
 * This adds significant overhead (6+ syscalls per loop iteration) and should
 * only be used for debugging performance issues.
 *
 * Symbol Prefixing:
 * Define IOS_PREFIXED to export functions with cemu_ prefix (for dual-backend iOS builds).
 */
#include "emu.h"  // Our adapter header

// Symbol prefixing for iOS dual-backend support
#ifdef IOS_PREFIXED
#define EMU_FUNC(name) cemu_##name
#else
#define EMU_FUNC(name) name
#endif

// CEmu headers
#include "asic.h"
#include "lcd.h"
#include "mem.h"
#include "cpu.h"
#include "keypad.h"
#include "schedule.h"
#include "backlight.h"
#include "bootver.h"
#include "cert.h"
#include "os/os.h"

#include <stdio.h>
#include <stdlib.h>
#include <stdarg.h>
#include <string.h>
#include <stdbool.h>
#include <unistd.h>  // For unlink()

// CEmu state size: ~4.6MB (4MB flash + 406KB RAM + peripherals)
// Use 5MB buffer for safety margin
#define CEMU_STATE_SIZE (5 * 1024 * 1024)
#define CEMU_IMAGE_VERSION 0xCECE001B

#ifdef CEMU_PERF_INSTRUMENTATION
#include <time.h>

// Timing stats (for performance debugging)
static uint64_t g_run_time_ns = 0;
static uint64_t g_draw_time_ns = 0;
static uint64_t g_cpu_exec_count = 0;
static uint64_t g_sched_time_ns = 0;
static uint64_t g_cpu_time_ns = 0;
static uint64_t g_signal_time_ns = 0;
static int g_frame_count = 0;
static int g_trace_enabled = 0;

static uint64_t get_time_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + ts.tv_nsec;
}
#endif /* CEMU_PERF_INSTRUMENTATION */

// Note: sched_repeat() already multiplies by tick_unit (160 at 48MHz),
// so we pass cycles directly, not base ticks.

// Wrapper state
struct Emu {
    bool initialized;
    uint32_t framebuffer[LCD_WIDTH * LCD_HEIGHT];
};

// Singleton - CEmu only supports one instance
static struct Emu* g_instance = NULL;
static emu_log_cb_t g_log_callback = NULL;
static char g_log_buffer[4096];

// GUI callback implementations (required by CEmu core)
void gui_console_clear(void) {
    // No-op
}

void gui_console_printf(const char *format, ...) {
    if (g_log_callback) {
        va_list args;
        va_start(args, format);
        vsnprintf(g_log_buffer, sizeof(g_log_buffer), format, args);
        va_end(args);
        g_log_callback(g_log_buffer);
    }
}

void gui_console_err_printf(const char *format, ...) {
    if (g_log_callback) {
        va_list args;
        va_start(args, format);
        vsnprintf(g_log_buffer, sizeof(g_log_buffer), format, args);
        va_end(args);
        g_log_callback(g_log_buffer);
    }
}

asic_rev_t gui_handle_reset(const boot_ver_t* boot_ver, asic_rev_t loaded_rev,
                            asic_rev_t default_rev, emu_device_t device, bool* python) {
    (void)boot_ver;
    (void)device;
    (void)python;
    if (loaded_rev != ASIC_REV_AUTO) {
        return loaded_rev;
    }
    return default_rev;
}

#ifdef DEBUG_SUPPORT
void gui_debug_open(int reason, uint32_t data) {
    (void)reason;
    (void)data;
}
void gui_debug_close(void) {}
#endif

// ============================================================
// CEmu emu.c functions reimplemented (to avoid symbol conflicts)
// ============================================================

// Internal: run emulation loop
static void cemu_run_internal(uint64_t ticks) {
    uint8_t signals;
#ifdef CEMU_PERF_INSTRUMENTATION
    uint64_t loop_count = 0;
    uint64_t t1, t2;
#endif

    sched.run_event_triggered = false;
    sched_repeat(SCHED_RUN, ticks);
    while (!((signals = cpu_clear_signals()) & CPU_SIGNAL_EXIT)) {
#ifdef CEMU_PERF_INSTRUMENTATION
        t1 = get_time_ns();
#endif
        if (signals & CPU_SIGNAL_ON_KEY) {
            keypad_on_check();
        }
        if (signals & CPU_SIGNAL_ANY_KEY) {
            keypad_any_check();
        }
#ifdef CEMU_PERF_INSTRUMENTATION
        g_signal_time_ns += get_time_ns() - t1;

        t1 = get_time_ns();
#endif
        sched_process_pending_events();
#ifdef CEMU_PERF_INSTRUMENTATION
        g_sched_time_ns += get_time_ns() - t1;
#endif

        if (signals & CPU_SIGNAL_RESET) {
            gui_console_printf("[CEmu] Reset triggered.\n");
            asic_reset();
        }
        if (sched.run_event_triggered) {
            break;
        }

#ifdef CEMU_PERF_INSTRUMENTATION
        t1 = get_time_ns();
#endif
        cpu_execute();
#ifdef CEMU_PERF_INSTRUMENTATION
        t2 = get_time_ns();
        g_cpu_time_ns += t2 - t1;

        loop_count++;
        g_cpu_exec_count++;
#endif
    }

#ifdef CEMU_PERF_INSTRUMENTATION
    // Log detailed stats every 10 frames
    if (g_trace_enabled && g_frame_count % 10 == 0) {
        gui_console_printf("[Trace] loops=%llu, halted=%d, PC=0x%06X\n",
            (unsigned long long)loop_count,
            cpu.halted,
            cpu.registers.PC);
    }
#endif
}

// Internal: load ROM from memory buffer (no temp file needed)
static int cemu_load_rom_from_memory(const uint8_t *rom_data, size_t rom_size) {
    bool gotType = false;
    uint16_t field_type;
    const uint8_t *outer;
    const uint8_t *current;
    const uint8_t *data;
    uint32_t outer_field_size;
    uint32_t data_field_size;
    emu_device_t device_type = TI84PCE;
    uint32_t offset;

    gui_console_printf("[CEmu] Loading ROM Image from memory (%zu bytes)...\n", rom_size);

    if (rom_size > SIZE_FLASH) {
        gui_console_err_printf("[CEmu] Invalid ROM size\n");
        return -1;
    }

    asic_free();
    asic_init();

    // Copy ROM data directly into flash memory
    memcpy(mem.flash.block, rom_data, rom_size);

    // Parse certificate fields to determine model
    for (offset = 0x20000U; offset < 0x40000U; offset += 0x10000U) {
        outer = mem.flash.block;

        if (cert_field_get(outer + offset, SIZE_FLASH - offset, &field_type, &outer, &outer_field_size)) break;
        if (field_type != 0x800F) continue;

        if (cert_field_get(outer, outer_field_size, &field_type, &data, &data_field_size)) break;
        if (field_type != 0x8012 || (data[0] != 0x13 && data[0] != 0x15)) break;
        const int model_id = data[0];

        data_field_size = outer_field_size - (data + data_field_size - outer);
        data = outer;
        if (cert_field_next(&data, &data_field_size)) break;
        current = data;
        if (cert_field_get(current, data_field_size, &field_type, &data, &data_field_size)) break;
        if (field_type != 0x8021) break;

        data_field_size = outer_field_size - (data + data_field_size - outer);
        data = current;
        if (cert_field_next(&data, &data_field_size)) break;
        current = data;
        if (cert_field_get(current, data_field_size, &field_type, &data, &data_field_size)) break;
        if (field_type != 0x8032) break;

        data_field_size = outer_field_size - (data + data_field_size - outer);
        data = current;
        if (cert_field_next(&data, &data_field_size)) break;
        current = data;
        if (cert_field_get(current, data_field_size, &field_type, &data, &data_field_size)) break;
        if (field_type != 0x80A1) break;

        data_field_size = outer_field_size - (data + data_field_size - outer);
        data = current;
        if (cert_field_next(&data, &data_field_size)) break;
        current = data;
        if (cert_field_get(current, data_field_size, &field_type, &data, &data_field_size)) break;
        if (field_type != 0x80C2) break;

        if (data[1] != 0 && data[1] != 1) break;
        const int device_id = data[1];

        gui_console_printf("[CEmu] Info from cert: Device type = 0x%02X. Model = 0x%02X.\n",
                           device_id, model_id);

        if (model_id == 0x15 && device_id == 1) {
            device_type = TI82AEP;
            gotType = true;
        } else if (model_id == 0x13 && device_id == 0) {
            device_type = TI84PCE;
            gotType = true;
        } else if (model_id == 0x13 && device_id == 1) {
            device_type = TI83PCE;
            gotType = true;
        }

        gui_console_printf("[CEmu] Loaded ROM Image.\n");
        break;
    }

    if (gotType) {
        set_device_type(device_type);
    } else {
        set_device_type(TI84PCE);
        gui_console_err_printf("[CEmu] Could not determine device type.\n");
    }

    asic_reset();
    return 0;
}

// ============================================================
// Public API Implementation
// ============================================================

Emu* EMU_FUNC(emu_create)(void) {
    if (g_instance != NULL) {
        return NULL;
    }

    g_instance = (struct Emu*)calloc(1, sizeof(struct Emu));
    if (!g_instance) {
        return NULL;
    }

    g_instance->initialized = false;
    return g_instance;
}

void EMU_FUNC(emu_destroy)(Emu* emu) {
    if (emu && emu == g_instance) {
        if (emu->initialized) {
            asic_free();
        }
        free(emu);
        g_instance = NULL;
    }
}

void EMU_FUNC(emu_set_log_callback)(emu_log_cb_t cb) {
    g_log_callback = cb;
}

int EMU_FUNC(emu_load_rom)(Emu* emu, const uint8_t* data, size_t len) {
    if (!emu || emu != g_instance || !data || len == 0) {
        return -1;
    }

    // Load ROM directly from memory (no temp file needed)
    int result = cemu_load_rom_from_memory(data, len);
    if (result != 0) {
        return -2;
    }

    // Set run rate to 48MHz
    sched_set_clock(CLOCK_RUN, 48000000);

    emu->initialized = true;
    return 0;
}

void EMU_FUNC(emu_reset)(Emu* emu) {
    if (emu && emu == g_instance && emu->initialized) {
        asic_reset();
    }
}

int EMU_FUNC(emu_run_cycles)(Emu* emu, int cycles) {
    if (!emu || emu != g_instance || !emu->initialized || cycles <= 0) {
        return 0;
    }

#ifdef CEMU_PERF_INSTRUMENTATION
    uint64_t start = get_time_ns();
#endif
    // Pass cycles directly - sched_repeat() multiplies by tick_unit internally
    cemu_run_internal((uint64_t)cycles);
#ifdef CEMU_PERF_INSTRUMENTATION
    g_run_time_ns += get_time_ns() - start;
#endif
    return cycles;
}

const uint32_t* EMU_FUNC(emu_framebuffer)(const Emu* emu, int* w, int* h) {
    // Always return valid dimensions (matches Rust implementation behavior)
    if (w) *w = LCD_WIDTH;
    if (h) *h = LCD_HEIGHT;

    if (!emu || emu != g_instance || !emu->initialized) {
        return NULL;
    }

#ifdef CEMU_PERF_INSTRUMENTATION
    uint64_t start = get_time_ns();
#endif
    emu_lcd_drawframe(((struct Emu*)emu)->framebuffer);
#ifdef CEMU_PERF_INSTRUMENTATION
    g_draw_time_ns += get_time_ns() - start;
    g_frame_count++;

    // Log stats every 60 frames
    if (g_frame_count >= 60) {
        uint64_t cpu_per_frame = g_cpu_exec_count / 60;
        uint64_t ns_per_exec = g_cpu_exec_count > 0 ? g_cpu_time_ns / g_cpu_exec_count : 0;
        gui_console_printf("[Perf] 60fr: total=%llums, cpu=%llums, sched=%llums, sig=%llums, draw=%llums\n",
            (unsigned long long)(g_run_time_ns / 1000000),
            (unsigned long long)(g_cpu_time_ns / 1000000),
            (unsigned long long)(g_sched_time_ns / 1000000),
            (unsigned long long)(g_signal_time_ns / 1000000),
            (unsigned long long)(g_draw_time_ns / 1000000));
        gui_console_printf("[Perf] exec_calls/fr=%llu, ns/exec=%llu\n",
            (unsigned long long)cpu_per_frame,
            (unsigned long long)ns_per_exec);
        g_run_time_ns = 0;
        g_draw_time_ns = 0;
        g_sched_time_ns = 0;
        g_cpu_time_ns = 0;
        g_signal_time_ns = 0;
        g_cpu_exec_count = 0;
        g_frame_count = 0;
        g_trace_enabled = 1;  // Enable detailed trace after first perf log
    }
#endif

    return emu->framebuffer;
}

void EMU_FUNC(emu_set_key)(Emu* emu, int row, int col, int down) {
    if (!emu || emu != g_instance || !emu->initialized) {
        return;
    }
    emu_keypad_event((unsigned int)row, (unsigned int)col, down != 0);
}

uint8_t EMU_FUNC(emu_get_backlight)(const Emu* emu) {
    if (!emu || emu != g_instance || !emu->initialized) {
        return 0;
    }
    return backlight.brightness;
}

int EMU_FUNC(emu_is_lcd_on)(const Emu* emu) {
    if (!emu || emu != g_instance || !emu->initialized) {
        return 0;
    }
    return (lcd.control & 1) ? 1 : 0;
}

size_t EMU_FUNC(emu_save_state_size)(const Emu* emu) {
    if (!emu || emu != g_instance || !emu->initialized) {
        return 0;
    }
    return CEMU_STATE_SIZE;
}

int EMU_FUNC(emu_save_state)(const Emu* emu, uint8_t* out, size_t cap) {
    if (!emu || emu != g_instance || !emu->initialized) return -1;
    if (!out || cap < CEMU_STATE_SIZE) return -101;  // Buffer too small

    // Use temp file as intermediary (CEmu only supports FILE* API)
    const char* temp_path = "/tmp/cemu_state_save.img";
    FILE* f = fopen(temp_path, "wb");
    if (!f) return -2;

    // Write version header
    uint32_t version = CEMU_IMAGE_VERSION;
    if (fwrite(&version, sizeof(version), 1, f) != 1) {
        fclose(f);
        unlink(temp_path);
        return -3;
    }

    // Save state via CEmu's asic_save()
    if (!asic_save(f)) {
        fclose(f);
        unlink(temp_path);
        return -4;
    }
    fclose(f);

    // Read temp file into output buffer
    f = fopen(temp_path, "rb");
    if (!f) {
        unlink(temp_path);
        return -5;
    }

    fseek(f, 0, SEEK_END);
    long size = ftell(f);
    fseek(f, 0, SEEK_SET);

    if (size <= 0 || (size_t)size > cap) {
        fclose(f);
        unlink(temp_path);
        return -101;
    }

    size_t bytes_read = fread(out, 1, (size_t)size, f);
    fclose(f);
    unlink(temp_path);

    gui_console_printf("[CEmu] Saved state: %zu bytes\n", bytes_read);
    return (bytes_read == (size_t)size) ? (int)bytes_read : -6;
}

int EMU_FUNC(emu_load_state)(Emu* emu, const uint8_t* data, size_t len) {
    if (!emu || emu != g_instance || !emu->initialized) return -1;
    if (!data || len < 8) return -105;  // Data corruption

    // Verify version header
    uint32_t version;
    memcpy(&version, data, sizeof(version));
    if (version != CEMU_IMAGE_VERSION) {
        gui_console_err_printf("[CEmu] State version mismatch: got 0x%08X, expected 0x%08X\n",
                               version, CEMU_IMAGE_VERSION);
        return -103;  // Version mismatch
    }

    // Write to temp file
    const char* temp_path = "/tmp/cemu_state_load.img";
    FILE* f = fopen(temp_path, "wb");
    if (!f) return -2;

    if (fwrite(data, 1, len, f) != len) {
        fclose(f);
        unlink(temp_path);
        return -3;
    }
    fclose(f);

    // Load via CEmu's asic_restore()
    f = fopen(temp_path, "rb");
    if (!f) {
        unlink(temp_path);
        return -4;
    }

    // Skip version header (already verified)
    fseek(f, sizeof(uint32_t), SEEK_SET);

    bool success = asic_restore(f);
    fclose(f);
    unlink(temp_path);

    if (success) {
        gui_console_printf("[CEmu] Restored state: %zu bytes\n", len);
        return 0;
    } else {
        gui_console_err_printf("[CEmu] Failed to restore state\n");
        return -105;  // Data corruption
    }
}

// ============================================================
// Backend API (for single-backend builds without bridge)
// ============================================================
#ifndef IOS_PREFIXED

const char* emu_backend_get_available(void) {
    return "cemu";
}

const char* emu_backend_get_current(void) {
    return "cemu";
}

int emu_backend_set(const char* name) {
    // Only "cemu" is available in single-backend build
    if (name && strcmp(name, "cemu") == 0) {
        return 0;
    }
    return -1;
}

int emu_backend_count(void) {
    return 1;
}

#endif /* !IOS_PREFIXED */
