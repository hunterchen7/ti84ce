#pragma once
#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct Emu Emu;
typedef void (*emu_log_cb_t)(const char* message);

// lifecycle
Emu* emu_create(void);
void emu_destroy(Emu*);
void emu_set_log_callback(emu_log_cb_t cb);

// ROM loading (bytes only)
int  emu_load_rom(Emu*, const uint8_t* data, size_t len); // 0 ok, else error code

void emu_reset(Emu*);

// Power on (simulate ON key press+release to wake from reset)
void emu_power_on(Emu*);

// execution
int  emu_run_cycles(Emu*, int cycles); // returns executed cycles

// framebuffer (owned by core), ARGB8888
const uint32_t* emu_framebuffer(const Emu*, int* w, int* h);

// input
void emu_set_key(Emu*, int row, int col, int down);

// backlight
uint8_t emu_get_backlight(const Emu*); // 0-255, 0 = off (screen black)

// LCD state - 1 if LCD is on (show content), 0 if LCD is off (show black)
int emu_is_lcd_on(const Emu*);

// optional save state (buffer-based)
size_t emu_save_state_size(const Emu*);
int    emu_save_state(const Emu*, uint8_t* out, size_t cap); // bytes written or <0
int    emu_load_state(Emu*, const uint8_t* data, size_t len);

#ifdef __cplusplus
}
#endif
