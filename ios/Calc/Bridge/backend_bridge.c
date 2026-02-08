/**
 * Backend Bridge Implementation for iOS
 *
 * This module provides runtime switching between Rust and CEmu backends.
 * Both backends are statically linked with prefixed symbols, and this
 * bridge forwards calls to the selected backend.
 *
 * Build configuration:
 * - HAS_RUST_BACKEND: Rust backend is available
 * - HAS_CEMU_BACKEND: CEmu backend is available
 */

#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <stdio.h>

// Forward declarations for the Emu struct
typedef struct Emu Emu;
typedef void (*emu_log_cb_t)(const char* message);

// Function pointer types for backend operations
typedef Emu* (*create_fn)(void);
typedef void (*destroy_fn)(Emu*);
typedef void (*set_log_callback_fn)(emu_log_cb_t);
typedef int (*load_rom_fn)(Emu*, const uint8_t*, size_t);
typedef void (*reset_fn)(Emu*);
typedef void (*power_on_fn)(Emu*);
typedef int (*run_cycles_fn)(Emu*, int);
typedef const uint32_t* (*framebuffer_fn)(const Emu*, int*, int*);
typedef void (*set_key_fn)(Emu*, int, int, int);
typedef uint8_t (*get_backlight_fn)(const Emu*);
typedef int (*is_lcd_on_fn)(const Emu*);
typedef size_t (*save_state_size_fn)(const Emu*);
typedef int (*save_state_fn)(const Emu*, uint8_t*, size_t);
typedef int (*load_state_fn)(Emu*, const uint8_t*, size_t);

// Backend interface structure
typedef struct {
    const char* name;
    create_fn create;
    destroy_fn destroy;
    set_log_callback_fn set_log_callback;
    load_rom_fn load_rom;
    reset_fn reset;
    power_on_fn power_on;
    run_cycles_fn run_cycles;
    framebuffer_fn framebuffer;
    set_key_fn set_key;
    get_backlight_fn get_backlight;
    is_lcd_on_fn is_lcd_on;
    save_state_size_fn save_state_size;
    save_state_fn save_state;
    load_state_fn load_state;
} BackendInterface;

// External declarations for Rust backend (prefixed)
#ifdef HAS_RUST_BACKEND
extern Emu* rust_emu_create(void);
extern void rust_emu_destroy(Emu*);
extern void rust_emu_set_log_callback(emu_log_cb_t);
extern int rust_emu_load_rom(Emu*, const uint8_t*, size_t);
extern void rust_emu_reset(Emu*);
extern void rust_emu_power_on(Emu*);
extern int rust_emu_run_cycles(Emu*, int);
extern const uint32_t* rust_emu_framebuffer(const Emu*, int*, int*);
extern void rust_emu_set_key(Emu*, int, int, int);
extern uint8_t rust_emu_get_backlight(const Emu*);
extern int rust_emu_is_lcd_on(const Emu*);
extern size_t rust_emu_save_state_size(const Emu*);
extern int rust_emu_save_state(const Emu*, uint8_t*, size_t);
extern int rust_emu_load_state(Emu*, const uint8_t*, size_t);

static const BackendInterface rust_backend = {
    .name = "rust",
    .create = rust_emu_create,
    .destroy = rust_emu_destroy,
    .set_log_callback = rust_emu_set_log_callback,
    .load_rom = rust_emu_load_rom,
    .reset = rust_emu_reset,
    .power_on = rust_emu_power_on,
    .run_cycles = rust_emu_run_cycles,
    .framebuffer = rust_emu_framebuffer,
    .set_key = rust_emu_set_key,
    .get_backlight = rust_emu_get_backlight,
    .is_lcd_on = rust_emu_is_lcd_on,
    .save_state_size = rust_emu_save_state_size,
    .save_state = rust_emu_save_state,
    .load_state = rust_emu_load_state,
};
#endif

// External declarations for CEmu backend (prefixed)
#ifdef HAS_CEMU_BACKEND
extern Emu* cemu_emu_create(void);
extern void cemu_emu_destroy(Emu*);
extern void cemu_emu_set_log_callback(emu_log_cb_t);
extern int cemu_emu_load_rom(Emu*, const uint8_t*, size_t);
extern void cemu_emu_reset(Emu*);
extern void cemu_emu_power_on(Emu*);
extern int cemu_emu_run_cycles(Emu*, int);
extern const uint32_t* cemu_emu_framebuffer(const Emu*, int*, int*);
extern void cemu_emu_set_key(Emu*, int, int, int);
extern uint8_t cemu_emu_get_backlight(const Emu*);
extern int cemu_emu_is_lcd_on(const Emu*);
extern size_t cemu_emu_save_state_size(const Emu*);
extern int cemu_emu_save_state(const Emu*, uint8_t*, size_t);
extern int cemu_emu_load_state(Emu*, const uint8_t*, size_t);

static const BackendInterface cemu_backend = {
    .name = "cemu",
    .create = cemu_emu_create,
    .destroy = cemu_emu_destroy,
    .set_log_callback = cemu_emu_set_log_callback,
    .load_rom = cemu_emu_load_rom,
    .reset = cemu_emu_reset,
    .power_on = cemu_emu_power_on,
    .run_cycles = cemu_emu_run_cycles,
    .framebuffer = cemu_emu_framebuffer,
    .set_key = cemu_emu_set_key,
    .get_backlight = cemu_emu_get_backlight,
    .is_lcd_on = cemu_emu_is_lcd_on,
    .save_state_size = cemu_emu_save_state_size,
    .save_state = cemu_emu_save_state,
    .load_state = cemu_emu_load_state,
};
#endif

// Current backend pointer
static const BackendInterface* current_backend = NULL;

// Available backends string (cached)
static char available_backends[64] = {0};

// Initialize available backends string
static void init_available_backends(void) {
    if (available_backends[0] != '\0') return;

    char* ptr = available_backends;
#ifdef HAS_RUST_BACKEND
    strcpy(ptr, "rust");
    ptr += 4;
#endif
#ifdef HAS_CEMU_BACKEND
    if (ptr != available_backends) {
        *ptr++ = ',';
    }
    strcpy(ptr, "cemu");
#endif
}

// Get the default backend
static const BackendInterface* get_default_backend(void) {
#ifdef HAS_RUST_BACKEND
    return &rust_backend;
#elif defined(HAS_CEMU_BACKEND)
    return &cemu_backend;
#else
    return NULL;
#endif
}

// Ensure a backend is selected
static void ensure_backend(void) {
    if (current_backend == NULL) {
        current_backend = get_default_backend();
    }
}

// ============================================================
// Backend Management API
// ============================================================

const char* emu_backend_get_available(void) {
    init_available_backends();
    return available_backends;
}

const char* emu_backend_get_current(void) {
    ensure_backend();
    return current_backend ? current_backend->name : NULL;
}

int emu_backend_count(void) {
    int count = 0;
#ifdef HAS_RUST_BACKEND
    count++;
#endif
#ifdef HAS_CEMU_BACKEND
    count++;
#endif
    return count;
}

int emu_backend_set(const char* name) {
    if (name == NULL) return -1;

#ifdef HAS_RUST_BACKEND
    if (strcmp(name, "rust") == 0) {
        current_backend = &rust_backend;
        return 0;
    }
#endif
#ifdef HAS_CEMU_BACKEND
    if (strcmp(name, "cemu") == 0) {
        current_backend = &cemu_backend;
        return 0;
    }
#endif

    return -1; // Backend not found
}

// ============================================================
// Standard Emulator API (forwards to current backend)
// ============================================================

Emu* emu_create(void) {
    ensure_backend();
    if (!current_backend) return NULL;
    return current_backend->create();
}

void emu_destroy(Emu* emu) {
    if (current_backend && emu) {
        current_backend->destroy(emu);
    }
}

void emu_set_log_callback(emu_log_cb_t cb) {
    ensure_backend();
    if (current_backend) {
        current_backend->set_log_callback(cb);
    }
}

int emu_load_rom(Emu* emu, const uint8_t* data, size_t len) {
    if (!current_backend || !emu) return -1;
    return current_backend->load_rom(emu, data, len);
}

void emu_reset(Emu* emu) {
    if (current_backend && emu) {
        current_backend->reset(emu);
    }
}

void emu_power_on(Emu* emu) {
    if (current_backend && emu) {
        current_backend->power_on(emu);
    }
}

int emu_run_cycles(Emu* emu, int cycles) {
    if (!current_backend || !emu) return 0;
    return current_backend->run_cycles(emu, cycles);
}

const uint32_t* emu_framebuffer(const Emu* emu, int* w, int* h) {
    if (!current_backend || !emu) {
        if (w) *w = 0;
        if (h) *h = 0;
        return NULL;
    }
    return current_backend->framebuffer(emu, w, h);
}

void emu_set_key(Emu* emu, int row, int col, int down) {
    if (current_backend && emu) {
        current_backend->set_key(emu, row, col, down);
    }
}

uint8_t emu_get_backlight(const Emu* emu) {
    if (!current_backend || !emu) return 0;
    return current_backend->get_backlight(emu);
}

int emu_is_lcd_on(const Emu* emu) {
    if (!current_backend || !emu) return 0;
    return current_backend->is_lcd_on(emu);
}

size_t emu_save_state_size(const Emu* emu) {
    if (!current_backend || !emu) return 0;
    return current_backend->save_state_size(emu);
}

int emu_save_state(const Emu* emu, uint8_t* out, size_t cap) {
    if (!current_backend || !emu) return -1;
    return current_backend->save_state(emu, out, cap);
}

int emu_load_state(Emu* emu, const uint8_t* data, size_t len) {
    if (!current_backend || !emu) return -1;
    return current_backend->load_state(emu, data, len);
}
