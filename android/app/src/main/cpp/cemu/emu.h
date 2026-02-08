/*
 * CEmu Adapter - provides same API as emu.h using CEmu backend
 *
 * This adapter wraps CEmu's global-state API to provide instance-based
 * interface matching our Rust emulator. Only one instance is supported
 * since CEmu uses global state.
 */
#ifndef CEMU_ADAPTER_H
#define CEMU_ADAPTER_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct Emu Emu;
typedef void (*emu_log_cb_t)(const char* message);

// Lifecycle
Emu* emu_create(void);
void emu_destroy(Emu* emu);
void emu_set_log_callback(emu_log_cb_t cb);

// ROM loading (bytes only)
int  emu_load_rom(Emu* emu, const uint8_t* data, size_t len);

void emu_reset(Emu* emu);

// Power on (simulate ON key press+release)
void emu_power_on(Emu* emu);

// Execution
int  emu_run_cycles(Emu* emu, int cycles);

// Framebuffer (owned by adapter), ARGB8888
const uint32_t* emu_framebuffer(const Emu* emu, int* w, int* h);

// Input
void emu_set_key(Emu* emu, int row, int col, int down);

// Backlight
uint8_t emu_get_backlight(const Emu* emu);

// LCD state
int emu_is_lcd_on(const Emu* emu);

// Save state
size_t emu_save_state_size(const Emu* emu);
int    emu_save_state(const Emu* emu, uint8_t* out, size_t cap);
int    emu_load_state(Emu* emu, const uint8_t* data, size_t len);

// Configuration
void backend_set_temp_dir(const char* path);

#ifdef __cplusplus
}
#endif

#endif // CEMU_ADAPTER_H
