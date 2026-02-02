/*
 * CEmu Wrapper - implements our emulator API using CEmu backend
 *
 * CEmu uses global state, so this wrapper creates a "virtual" instance
 * that just wraps the global state. Only one instance can be active.
 */
#include "cemu_wrapper.h"
#include "../../cemu-ref/core/emu.h"
#include "../../cemu-ref/core/asic.h"
#include "../../cemu-ref/core/lcd.h"
#include "../../cemu-ref/core/mem.h"
#include "../../cemu-ref/core/cpu.h"
#include "../../cemu-ref/core/keypad.h"
#include "../../cemu-ref/core/schedule.h"
#include "../../cemu-ref/core/backlight.h"

#include <stdio.h>
#include <stdlib.h>
#include <stdarg.h>
#include <string.h>
#include <stdbool.h>

// Tick conversion: at 48MHz, 160 base ticks = 1 CPU cycle
#define TICKS_PER_CYCLE 160

// Wrapper state
struct WrapEmu {
    bool initialized;
    uint32_t framebuffer[LCD_WIDTH * LCD_HEIGHT];
};

// Singleton - CEmu only supports one instance
static struct WrapEmu* g_instance = NULL;
static wrap_log_cb_t g_log_callback = NULL;
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

// API Implementation

WrapEmu* wrap_emu_create(void) {
    if (g_instance != NULL) {
        // Only one instance allowed
        return NULL;
    }

    g_instance = (struct WrapEmu*)calloc(1, sizeof(struct WrapEmu));
    if (!g_instance) {
        return NULL;
    }

    g_instance->initialized = false;
    return g_instance;
}

void wrap_emu_destroy(WrapEmu* emu) {
    if (emu && emu == g_instance) {
        if (emu->initialized) {
            asic_free();
        }
        free(emu);
        g_instance = NULL;
    }
}

void wrap_emu_set_log_callback(wrap_log_cb_t cb) {
    g_log_callback = cb;
}

int wrap_emu_load_rom(WrapEmu* emu, const uint8_t* data, size_t len) {
    if (!emu || emu != g_instance || !data || len == 0) {
        return -1;
    }

    // CEmu's emu_load expects a file path, so we need to write to a temp file
    const char* temp_path = "/tmp/cemu_temp_rom.rom";
    FILE* f = fopen(temp_path, "wb");
    if (!f) {
        return -2;
    }

    if (fwrite(data, 1, len, f) != len) {
        fclose(f);
        return -3;
    }
    fclose(f);

    // Load via CEmu
    emu_state_t state = emu_load(EMU_DATA_ROM, temp_path);
    if (state != EMU_STATE_VALID) {
        return -4;
    }

    // Set run rate to 48MHz
    emu_set_run_rate(48000000);

    emu->initialized = true;
    return 0;
}

void wrap_emu_reset(WrapEmu* emu) {
    if (emu && emu == g_instance && emu->initialized) {
        asic_reset();
    }
}

int wrap_emu_run_cycles(WrapEmu* emu, int cycles) {
    if (!emu || emu != g_instance || !emu->initialized || cycles <= 0) {
        return 0;
    }

    // Convert cycles to ticks
    uint64_t ticks = (uint64_t)cycles * TICKS_PER_CYCLE;

    // Run emulation
    emu_run(ticks);

    return cycles;
}

const uint32_t* wrap_emu_framebuffer(const WrapEmu* emu, int* w, int* h) {
    if (!emu || emu != g_instance || !emu->initialized) {
        if (w) *w = 0;
        if (h) *h = 0;
        return NULL;
    }

    if (w) *w = LCD_WIDTH;
    if (h) *h = LCD_HEIGHT;

    // Copy framebuffer (emu_lcd_drawframe modifies buffer, so cast away const)
    emu_lcd_drawframe(((struct WrapEmu*)emu)->framebuffer);

    return emu->framebuffer;
}

void wrap_emu_set_key(WrapEmu* emu, int row, int col, int down) {
    if (!emu || emu != g_instance || !emu->initialized) {
        return;
    }

    emu_keypad_event((unsigned int)row, (unsigned int)col, down != 0);
}

uint8_t wrap_emu_get_backlight(const WrapEmu* emu) {
    if (!emu || emu != g_instance || !emu->initialized) {
        return 0;
    }

    return backlight.brightness;
}

int wrap_emu_is_lcd_on(const WrapEmu* emu) {
    if (!emu || emu != g_instance || !emu->initialized) {
        return 0;
    }

    // Check LCD control register bit 0 (enable)
    return (lcd.control & 1) ? 1 : 0;
}

size_t wrap_emu_save_state_size(const WrapEmu* emu) {
    (void)emu;
    // CEmu save state is complex, estimate a large size
    // In practice, would need to implement proper serialization
    return 0; // Not implemented
}

int wrap_emu_save_state(const WrapEmu* emu, uint8_t* out, size_t cap) {
    (void)emu;
    (void)out;
    (void)cap;
    // Not implemented - would need to serialize CEmu state to buffer
    return -1;
}

int wrap_emu_load_state(WrapEmu* emu, const uint8_t* data, size_t len) {
    (void)emu;
    (void)data;
    (void)len;
    // Not implemented - would need to deserialize CEmu state from buffer
    return -1;
}

// Debug functions

uint32_t wrap_emu_get_pc(const WrapEmu* emu) {
    if (!emu || emu != g_instance || !emu->initialized) {
        return 0;
    }
    return cpu.registers.PC;
}

uint8_t wrap_emu_peek_byte(const WrapEmu* emu, uint32_t addr) {
    if (!emu || emu != g_instance || !emu->initialized) {
        return 0;
    }
    return mem_peek_byte(addr);
}
