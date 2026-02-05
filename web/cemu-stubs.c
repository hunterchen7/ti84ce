/**
 * GUI stubs for CEmu WASM build
 * These functions are called by the CEmu core but not needed for headless/web operation
 */

#include <stdint.h>
#include <stdbool.h>
#include <stdio.h>

#ifdef __EMSCRIPTEN__
#include <emscripten.h>
#endif

/* Forward declarations from CEmu */
extern void emu_run(uint64_t ticks);
extern int emu_load(int type, const char *path);
extern void emu_reset(void);
extern void emu_set_lcd_dma(int enable);
extern void emu_set_lcd_gamma(int enable);
extern void debug_init(void);
extern void debug_flag(int flag, int value);

/* From emu.h - emu_data_t enum values */
#define EMU_DATA_IMAGE 0
#define EMU_DATA_ROM 1
#define EMU_DATA_RAM 2
#define EMU_STATE_VALID 0

/* From debug.h */
#define DBG_SOFT_COMMANDS 3

/* From emu.h - device types */
typedef enum {
    EMU_DEVICE_83PCE,
    EMU_DEVICE_83PCE_EP,
    EMU_DEVICE_84PCE,
    EMU_DEVICE_84PCE_PE,
    EMU_DEVICE_84PCE_T,
    EMU_DEVICE_84PCE_TPE,
    EMU_DEVICE_82AEP,
    EMU_DEVICE_84PCEPY,
    EMU_DEVICE_84PCEPE_PY,
    EMU_DEVICE_84PCE_T_PY,
    EMU_DEVICE_UNKNOWN
} emu_device_t;

/* From asic.h - ASIC revision */
typedef enum {
    ASIC_REV_AUTO,
    ASIC_REV_A,
    ASIC_REV_I,
    ASIC_REV_M
} asic_rev_t;

/* From bootver.h - boot version struct */
typedef struct {
    uint8_t major;
    uint8_t minor;
    uint16_t revision;
    uint32_t magic;
} boot_ver_t;

/**
 * Handle reset - return the loaded revision as-is (no user interaction in web)
 */
asic_rev_t gui_handle_reset(const boot_ver_t* boot_ver, asic_rev_t loaded_rev,
                            asic_rev_t default_rev, emu_device_t device, bool* python) {
    (void)boot_ver;
    (void)default_rev;
    (void)device;
    if (python) {
        *python = false;
    }
    return loaded_rev;
}

/**
 * Initialize the emulator without starting the main loop.
 * Returns 0 on success, non-zero on failure.
 */
#ifdef __EMSCRIPTEN__
int EMSCRIPTEN_KEEPALIVE emu_init(const char *rom_path) {
    int success = emu_load(EMU_DATA_ROM, rom_path);

    if (success == EMU_STATE_VALID) {
        /* Enable LCD DMA and gamma for proper framebuffer rendering */
        emu_set_lcd_dma(1);
        emu_set_lcd_gamma(1);
        return 0;
    }
    return -1;
}

/**
 * Exported step function to run the emulator for N frames.
 * This allows manual stepping in environments where emscripten_set_main_loop
 * doesn't work (like Node.js).
 */
void EMSCRIPTEN_KEEPALIVE emu_step(unsigned int frames) {
    for (unsigned int i = 0; i < frames; i++) {
        emu_run((uint64_t)1);
    }
}

/* Forward declarations for save/load state */
extern bool emu_save(int type, const char *path);

/**
 * Get the size needed for state buffer.
 * CEmu state images are approximately 5MB.
 */
int EMSCRIPTEN_KEEPALIVE emu_save_state_size(void) {
    /* CEmu state size is roughly 5MB, return a safe upper bound */
    return 5 * 1024 * 1024;
}

/**
 * Save emulator state to a memory buffer.
 * Returns 0 on success, non-zero on failure.
 */
int EMSCRIPTEN_KEEPALIVE emu_save_state(uint8_t *buffer, int buffer_size) {
    const char *temp_path = "/tmp/state.img";

    /* Save state to temp file using CEmu's emu_save */
    if (!emu_save(EMU_DATA_IMAGE, temp_path)) {
        return -1;
    }

    /* Read temp file into buffer */
    FILE *f = fopen(temp_path, "rb");
    if (!f) {
        return -2;
    }

    /* Get file size */
    fseek(f, 0, SEEK_END);
    long size = ftell(f);
    fseek(f, 0, SEEK_SET);

    if (size > buffer_size) {
        fclose(f);
        return -3; /* Buffer too small */
    }

    /* Read into buffer */
    size_t read = fread(buffer, 1, size, f);
    fclose(f);

    /* Remove temp file */
    remove(temp_path);

    if (read != (size_t)size) {
        return -4;
    }

    return (int)size; /* Return actual state size */
}

/**
 * Load emulator state from a memory buffer.
 * Returns 0 on success, non-zero on failure.
 */
int EMSCRIPTEN_KEEPALIVE emu_load_state(const uint8_t *buffer, int size) {
    const char *temp_path = "/tmp/state.img";

    /* Write buffer to temp file */
    FILE *f = fopen(temp_path, "wb");
    if (!f) {
        return -1;
    }

    size_t written = fwrite(buffer, 1, size, f);
    fclose(f);

    if (written != (size_t)size) {
        remove(temp_path);
        return -2;
    }

    /* Load state from temp file using CEmu's emu_load */
    int result = emu_load(EMU_DATA_IMAGE, temp_path);

    /* Remove temp file */
    remove(temp_path);

    return (result == EMU_STATE_VALID) ? 0 : -3;
}
#endif
