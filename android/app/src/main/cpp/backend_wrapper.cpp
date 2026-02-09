/**
 * Backend Wrapper
 *
 * This file is compiled into each backend .so (libemu_rust.so, libemu_cemu.so).
 * It exposes the emu.h functions with a "backend_" prefix so the JNI loader
 * can dynamically load and call them.
 *
 * BACKEND_NAME is defined at compile time to identify which backend this is.
 */

#include <android/log.h>
#include "emu.h"

#define LOG_TAG "EmuBackend"
#define LOGI(...) __android_log_print(ANDROID_LOG_INFO, LOG_TAG, __VA_ARGS__)

#ifndef BACKEND_NAME
#define BACKEND_NAME "unknown"
#endif

extern "C" {

// Return the backend name for identification
const char* backend_get_name() {
    return BACKEND_NAME;
}

// Wrapped API functions - these just forward to the actual implementation
Emu* backend_create() {
    LOGI("Creating emulator instance (backend: %s)", BACKEND_NAME);
    return emu_create();
}

void backend_destroy(Emu* emu) {
    LOGI("Destroying emulator instance (backend: %s)", BACKEND_NAME);
    emu_destroy(emu);
}

void backend_set_log_callback(emu_log_cb_t cb) {
    emu_set_log_callback(cb);
}

int backend_load_rom(Emu* emu, const uint8_t* data, size_t len) {
    return emu_load_rom(emu, data, len);
}

void backend_reset(Emu* emu) {
    emu_reset(emu);
}

void backend_power_on(Emu* emu) {
    emu_power_on(emu);
}

int backend_run_cycles(Emu* emu, int cycles) {
    return emu_run_cycles(emu, cycles);
}

const uint32_t* backend_framebuffer(const Emu* emu, int* w, int* h) {
    return emu_framebuffer(emu, w, h);
}

void backend_set_key(Emu* emu, int row, int col, int down) {
    emu_set_key(emu, row, col, down);
}

uint8_t backend_get_backlight(const Emu* emu) {
    return emu_get_backlight(emu);
}

int backend_is_lcd_on(const Emu* emu) {
    return emu_is_lcd_on(emu);
}

size_t backend_save_state_size(const Emu* emu) {
    return emu_save_state_size(emu);
}

int backend_save_state(const Emu* emu, uint8_t* out, size_t cap) {
    return emu_save_state(emu, out, cap);
}

int backend_load_state(Emu* emu, const uint8_t* data, size_t len) {
    return emu_load_state(emu, data, len);
}

} // extern "C"
