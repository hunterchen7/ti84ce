/**
 * Backend Bridge API for iOS Dual-Backend Support
 *
 * This header defines the interface for runtime backend switching.
 * Both Rust and CEmu backends are statically linked, and this bridge
 * allows switching between them at runtime via function pointers.
 */

#pragma once
#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct Emu Emu;
typedef void (*emu_log_cb_t)(const char* message);

/**
 * Backend information
 */
const char* emu_backend_get_available(void);  // Returns comma-separated list: "rust,cemu" or "rust" or "cemu"
const char* emu_backend_get_current(void);    // Returns current backend name or NULL
int emu_backend_set(const char* name);        // Switch to named backend, returns 0 on success
int emu_backend_count(void);                  // Number of available backends

/**
 * Standard emulator API (forwards to current backend)
 */
Emu* emu_create(void);
void emu_destroy(Emu*);
void emu_set_log_callback(emu_log_cb_t cb);

int  emu_load_rom(Emu*, const uint8_t* data, size_t len);
void emu_reset(Emu*);
void emu_power_on(Emu*);

int  emu_run_cycles(Emu*, int cycles);

const uint32_t* emu_framebuffer(const Emu*, int* w, int* h);

void emu_set_key(Emu*, int row, int col, int down);

uint8_t emu_get_backlight(const Emu*);
int emu_is_lcd_on(const Emu*);

size_t emu_save_state_size(const Emu*);
int    emu_save_state(const Emu*, uint8_t* out, size_t cap);
int    emu_load_state(Emu*, const uint8_t* data, size_t len);

#ifdef __cplusplus
}
#endif
