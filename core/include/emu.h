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

// execution
int  emu_run_cycles(Emu*, int cycles); // returns executed cycles

// framebuffer (owned by core), ARGB8888
const uint32_t* emu_framebuffer(const Emu*, int* w, int* h);

// input
void emu_set_key(Emu*, int row, int col, int down);

// optional save state (buffer-based)
size_t emu_save_state_size(const Emu*);
int    emu_save_state(const Emu*, uint8_t* out, size_t cap); // bytes written or <0
int    emu_load_state(Emu*, const uint8_t* data, size_t len);

#ifdef __cplusplus
}
#endif
