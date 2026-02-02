/*
 * Test program for CEmu wrapper API
 * Tests that our API works with CEmu backend
 */
#include "cemu_wrapper.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define LCD_WIDTH 320
#define LCD_HEIGHT 240

// Log callback
void log_callback(const char* message) {
    printf("%s", message);
}

// Save framebuffer as PPM
void save_ppm(const uint32_t* fb, int w, int h, const char* filename) {
    FILE* f = fopen(filename, "wb");
    if (!f) {
        fprintf(stderr, "Failed to open %s\n", filename);
        return;
    }

    fprintf(f, "P6\n%d %d\n255\n", w, h);
    for (int i = 0; i < w * h; i++) {
        uint32_t pixel = fb[i];
        uint8_t r = (pixel >> 16) & 0xFF;
        uint8_t g = (pixel >> 8) & 0xFF;
        uint8_t b = pixel & 0xFF;
        fputc(r, f);
        fputc(g, f);
        fputc(b, f);
    }
    fclose(f);
    printf("Saved: %s\n", filename);
}

int main(int argc, char* argv[]) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <rom_file>\n", argv[0]);
        return 1;
    }

    // Read ROM file into memory
    FILE* f = fopen(argv[1], "rb");
    if (!f) {
        fprintf(stderr, "Failed to open ROM: %s\n", argv[1]);
        return 1;
    }

    fseek(f, 0, SEEK_END);
    size_t rom_size = ftell(f);
    rewind(f);

    uint8_t* rom_data = malloc(rom_size);
    if (!rom_data) {
        fprintf(stderr, "Failed to allocate ROM buffer\n");
        fclose(f);
        return 1;
    }

    if (fread(rom_data, 1, rom_size, f) != rom_size) {
        fprintf(stderr, "Failed to read ROM\n");
        free(rom_data);
        fclose(f);
        return 1;
    }
    fclose(f);

    printf("ROM loaded: %zu bytes\n", rom_size);

    // Set log callback
    wrap_emu_set_log_callback(log_callback);

    // Create emulator
    WrapEmu* emu = wrap_emu_create();
    if (!emu) {
        fprintf(stderr, "Failed to create emulator\n");
        free(rom_data);
        return 1;
    }
    printf("Emulator created\n");

    // Load ROM
    int result = wrap_emu_load_rom(emu, rom_data, rom_size);
    if (result != 0) {
        fprintf(stderr, "Failed to load ROM: %d\n", result);
        wrap_emu_destroy(emu);
        free(rom_data);
        return 1;
    }
    printf("ROM loaded into emulator\n");

    // Run for 70M cycles (enough for boot)
    int total_cycles = 70000000;
    int chunk = 10000000;
    int screenshot = 0;

    for (int i = 0; i < total_cycles; i += chunk) {
        int to_run = chunk;
        if (i + to_run > total_cycles) {
            to_run = total_cycles - i;
        }

        int executed = wrap_emu_run_cycles(emu, to_run);
        printf("Executed %d cycles (total: %d/%d)\n", executed, i + executed, total_cycles);

        // Get framebuffer and save screenshot
        int w, h;
        const uint32_t* fb = wrap_emu_framebuffer(emu, &w, &h);
        if (fb && w > 0 && h > 0 && screenshot < 3) {
            char filename[64];
            snprintf(filename, sizeof(filename), "wrapper_screen_%d.ppm", screenshot++);
            save_ppm(fb, w, h, filename);
        }
    }

    // Final screenshot
    int w, h;
    const uint32_t* fb = wrap_emu_framebuffer(emu, &w, &h);
    if (fb && w > 0 && h > 0) {
        save_ppm(fb, w, h, "wrapper_screen_final.ppm");
    }

    // Debug info
    printf("\nFinal state:\n");
    printf("  PC: 0x%06X\n", wrap_emu_get_pc(emu));
    printf("  MathPrint flag (0xD000C4): 0x%02X\n", wrap_emu_peek_byte(emu, 0xD000C4));
    printf("  Backlight: %d\n", wrap_emu_get_backlight(emu));
    printf("  LCD on: %d\n", wrap_emu_is_lcd_on(emu));

    // Cleanup
    wrap_emu_destroy(emu);
    free(rom_data);

    printf("\nTest complete!\n");
    return 0;
}
