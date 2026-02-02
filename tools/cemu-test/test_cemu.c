/*
 * Test program for CEmu core library
 * Implements GUI stubs and tests ROM loading/execution
 */
#include <stdio.h>
#include <stdlib.h>
#include <stdarg.h>
#include <string.h>
#include <stdint.h>

// CEmu headers
#include "../../cemu-ref/core/emu.h"
#include "../../cemu-ref/core/asic.h"
#include "../../cemu-ref/core/lcd.h"
#include "../../cemu-ref/core/mem.h"
#include "../../cemu-ref/core/cpu.h"
#include "../../cemu-ref/core/schedule.h"

// GUI callback implementations (required by CEmu core)
void gui_console_clear(void) {
    // No-op for testing
}

void gui_console_printf(const char *format, ...) {
    va_list args;
    va_start(args, format);
    vfprintf(stdout, format, args);
    va_end(args);
}

void gui_console_err_printf(const char *format, ...) {
    va_list args;
    va_start(args, format);
    vfprintf(stderr, format, args);
    va_end(args);
}

asic_rev_t gui_handle_reset(const boot_ver_t* boot_ver, asic_rev_t loaded_rev,
                            asic_rev_t default_rev, emu_device_t device, bool* python) {
    (void)boot_ver;
    (void)device;
    (void)python;
    // Return the loaded revision, or default if auto
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

// Save LCD framebuffer as PPM image
void save_lcd_ppm(const char* filename) {
    uint32_t framebuffer[LCD_WIDTH * LCD_HEIGHT];
    emu_lcd_drawframe(framebuffer);

    FILE* f = fopen(filename, "wb");
    if (!f) {
        fprintf(stderr, "Failed to open %s for writing\n", filename);
        return;
    }

    fprintf(f, "P6\n%d %d\n255\n", LCD_WIDTH, LCD_HEIGHT);
    for (int i = 0; i < LCD_WIDTH * LCD_HEIGHT; i++) {
        uint32_t pixel = framebuffer[i];
        // ARGB8888 format
        uint8_t r = (pixel >> 16) & 0xFF;
        uint8_t g = (pixel >> 8) & 0xFF;
        uint8_t b = pixel & 0xFF;
        fputc(r, f);
        fputc(g, f);
        fputc(b, f);
    }
    fclose(f);
    printf("Saved LCD to %s\n", filename);
}

int main(int argc, char* argv[]) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <rom_file> [cycles]\n", argv[0]);
        return 1;
    }

    const char* rom_path = argv[1];
    uint64_t cycles = 70000000; // Default: 70M cycles (enough for boot)
    if (argc >= 3) {
        cycles = strtoull(argv[2], NULL, 10);
    }

    printf("Loading ROM: %s\n", rom_path);

    // Load ROM
    emu_state_t state = emu_load(EMU_DATA_ROM, rom_path);
    if (state != EMU_STATE_VALID) {
        fprintf(stderr, "Failed to load ROM (state=%d)\n", state);
        return 1;
    }

    printf("ROM loaded successfully, device type: %d\n", get_device_type());

    // Set run rate to 48MHz
    if (!emu_set_run_rate(48000000)) {
        fprintf(stderr, "Failed to set run rate\n");
        return 1;
    }

    printf("Running %llu cycles...\n", (unsigned long long)cycles);

    // CEmu uses ticks, not cycles. At 48MHz, 160 ticks = 1 cycle
    // ticks = cycles * 160
    uint64_t ticks = cycles * 160;

    // Run in chunks to allow periodic screenshots
    uint64_t chunk_size = 10000000 * 160; // 10M cycles per chunk
    uint64_t ticks_run = 0;
    int screenshot_num = 0;

    while (ticks_run < ticks) {
        uint64_t run_ticks = chunk_size;
        if (ticks_run + run_ticks > ticks) {
            run_ticks = ticks - ticks_run;
        }

        emu_run(run_ticks);
        ticks_run += run_ticks;

        printf("Progress: %llu / %llu cycles (%.1f%%)\n",
               (unsigned long long)(ticks_run / 160),
               (unsigned long long)cycles,
               100.0 * ticks_run / ticks);

        // Take periodic screenshots
        if (screenshot_num < 5) {
            char filename[64];
            snprintf(filename, sizeof(filename), "cemu_screen_%d.ppm", screenshot_num++);
            save_lcd_ppm(filename);
        }
    }

    // Final screenshot
    save_lcd_ppm("cemu_screen_final.ppm");

    printf("Emulation complete!\n");
    printf("Total cycles: %llu\n", (unsigned long long)sched_total_cycles());
    printf("PC: 0x%06X\n", cpu.registers.PC);

    // Cleanup
    asic_free();

    return 0;
}
