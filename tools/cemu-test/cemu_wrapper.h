/*
 * CEmu Wrapper - implements our emulator API using CEmu backend
 *
 * This provides API compatibility so we can swap between our Rust
 * implementation and CEmu for debugging/comparison.
 *
 * Note: Functions are prefixed with "wrap_" to avoid conflicts with
 * CEmu's own emu_* functions.
 */
#ifndef CEMU_WRAPPER_H
#define CEMU_WRAPPER_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct WrapEmu WrapEmu;
typedef void (*wrap_log_cb_t)(const char* message);

// Lifecycle
WrapEmu* wrap_emu_create(void);
void wrap_emu_destroy(WrapEmu* emu);
void wrap_emu_set_log_callback(wrap_log_cb_t cb);

// ROM loading (bytes only)
int  wrap_emu_load_rom(WrapEmu* emu, const uint8_t* data, size_t len); // 0 ok, else error code

void wrap_emu_reset(WrapEmu* emu);

// Execution
int  wrap_emu_run_cycles(WrapEmu* emu, int cycles); // returns executed cycles

// Framebuffer (owned by wrapper), ARGB8888
const uint32_t* wrap_emu_framebuffer(const WrapEmu* emu, int* w, int* h);

// Input
void wrap_emu_set_key(WrapEmu* emu, int row, int col, int down);

// Backlight
uint8_t wrap_emu_get_backlight(const WrapEmu* emu); // 0-255, 0 = off (screen black)

// LCD state - 1 if LCD is on (show content), 0 if LCD is off (show black)
int wrap_emu_is_lcd_on(const WrapEmu* emu);

// Optional save state (buffer-based)
size_t wrap_emu_save_state_size(const WrapEmu* emu);
int    wrap_emu_save_state(const WrapEmu* emu, uint8_t* out, size_t cap); // bytes written or <0
int    wrap_emu_load_state(WrapEmu* emu, const uint8_t* data, size_t len);

// Debug functions specific to CEmu wrapper
uint32_t wrap_emu_get_pc(const WrapEmu* emu);
uint8_t wrap_emu_peek_byte(const WrapEmu* emu, uint32_t addr);

#ifdef __cplusplus
}
#endif

#endif // CEMU_WRAPPER_H
