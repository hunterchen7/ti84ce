/*
 * CEmu Adapter Implementation
 *
 * Wraps CEmu's global-state API to provide instance-based interface.
 * Since emu.c has conflicting function names, we implement needed functionality here.
 */
#include "emu.h"  // Our adapter header

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

// Tick conversion: at 48MHz, 160 base ticks = 1 CPU cycle
#define TICKS_PER_CYCLE 160

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
    sched.run_event_triggered = false;
    sched_repeat(SCHED_RUN, ticks);
    while (!((signals = cpu_clear_signals()) & CPU_SIGNAL_EXIT)) {
        if (signals & CPU_SIGNAL_ON_KEY) {
            keypad_on_check();
        }
        if (signals & CPU_SIGNAL_ANY_KEY) {
            keypad_any_check();
        }
        sched_process_pending_events();
        if (signals & CPU_SIGNAL_RESET) {
            gui_console_printf("[CEmu] Reset triggered.\n");
            asic_reset();
        }
        if (sched.run_event_triggered) {
            break;
        }
        cpu_execute();
    }
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

Emu* emu_create(void) {
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

void emu_destroy(Emu* emu) {
    if (emu && emu == g_instance) {
        if (emu->initialized) {
            asic_free();
        }
        free(emu);
        g_instance = NULL;
    }
}

void emu_set_log_callback(emu_log_cb_t cb) {
    g_log_callback = cb;
}

int emu_load_rom(Emu* emu, const uint8_t* data, size_t len) {
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

void emu_reset(Emu* emu) {
    if (emu && emu == g_instance && emu->initialized) {
        asic_reset();
    }
}

int emu_run_cycles(Emu* emu, int cycles) {
    if (!emu || emu != g_instance || !emu->initialized || cycles <= 0) {
        return 0;
    }

    uint64_t ticks = (uint64_t)cycles * TICKS_PER_CYCLE;
    cemu_run_internal(ticks);
    return cycles;
}

const uint32_t* emu_framebuffer(const Emu* emu, int* w, int* h) {
    // Always return valid dimensions (matches Rust implementation behavior)
    if (w) *w = LCD_WIDTH;
    if (h) *h = LCD_HEIGHT;

    if (!emu || emu != g_instance || !emu->initialized) {
        return NULL;
    }

    emu_lcd_drawframe(((struct Emu*)emu)->framebuffer);
    return emu->framebuffer;
}

void emu_set_key(Emu* emu, int row, int col, int down) {
    if (!emu || emu != g_instance || !emu->initialized) {
        return;
    }
    emu_keypad_event((unsigned int)row, (unsigned int)col, down != 0);
}

uint8_t emu_get_backlight(const Emu* emu) {
    if (!emu || emu != g_instance || !emu->initialized) {
        return 0;
    }
    return backlight.brightness;
}

int emu_is_lcd_on(const Emu* emu) {
    if (!emu || emu != g_instance || !emu->initialized) {
        return 0;
    }
    return (lcd.control & 1) ? 1 : 0;
}

size_t emu_save_state_size(const Emu* emu) {
    (void)emu;
    return 0;
}

int emu_save_state(const Emu* emu, uint8_t* out, size_t cap) {
    (void)emu; (void)out; (void)cap;
    return -1;
}

int emu_load_state(Emu* emu, const uint8_t* data, size_t len) {
    (void)emu; (void)data; (void)len;
    return -1;
}
