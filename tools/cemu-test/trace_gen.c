/*
 * CEmu Trace Generator
 * Generates CPU trace output in the same format as our Rust emulator for comparison
 *
 * Output format (space-separated):
 *   step cycles PC SP AF BC DE HL IX IY ADL IFF1 IFF2 IM HALT opcode
 */
#include "../../cemu-ref/core/emu.h"
#include "../../cemu-ref/core/asic.h"
#include "../../cemu-ref/core/lcd.h"
#include "../../cemu-ref/core/mem.h"
#include "../../cemu-ref/core/cpu.h"
#include "../../cemu-ref/core/keypad.h"
#include "../../cemu-ref/core/schedule.h"
#include "../../cemu-ref/core/backlight.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdarg.h>
#include <stdbool.h>

#define LCD_WIDTH 320
#define LCD_HEIGHT 240

// Silence CEmu console output
void gui_console_clear(void) {}
void gui_console_printf(const char *format, ...) { (void)format; }
void gui_console_err_printf(const char *format, ...) { (void)format; }

asic_rev_t gui_handle_reset(const boot_ver_t* boot_ver, asic_rev_t loaded_rev,
                            asic_rev_t default_rev, emu_device_t device, bool* python) {
    (void)boot_ver; (void)device; (void)python;
    return (loaded_rev != ASIC_REV_AUTO) ? loaded_rev : default_rev;
}

#ifdef DEBUG_SUPPORT
void gui_debug_open(int reason, uint32_t data) { (void)reason; (void)data; }
void gui_debug_close(void) {}
#endif

void log_trace_line(FILE* out, uint64_t step, uint64_t cycles) {
    // Get register values
    uint32_t pc = cpu.registers.PC;
    uint32_t sp = cpu.ADL ? cpu.registers.SPL : cpu.registers.SPS;
    uint16_t af = cpu.registers.AF;
    uint32_t bc = cpu.registers.BC;
    uint32_t de = cpu.registers.DE;
    uint32_t hl = cpu.registers.HL;
    uint32_t ix = cpu.registers.IX;
    uint32_t iy = cpu.registers.IY;
    int adl = cpu.ADL ? 1 : 0;
    int iff1 = cpu.IEF1 ? 1 : 0;
    int iff2 = cpu.IEF2 ? 1 : 0;
    int im = cpu.IM;
    int halted = cpu.halted ? 1 : 0;

    // Read opcode bytes at PC
    uint8_t op1 = mem_peek_byte(pc);
    uint8_t op2 = mem_peek_byte(pc + 1);
    uint8_t op3 = mem_peek_byte(pc + 2);
    uint8_t op4 = mem_peek_byte(pc + 3);

    // Format opcode string (match Rust format)
    char op_str[16];
    if (op1 == 0xDD || op1 == 0xFD) {
        if (op2 == 0xCB) {
            snprintf(op_str, sizeof(op_str), "%02X%02X%02X%02X", op1, op2, op3, op4);
        } else {
            snprintf(op_str, sizeof(op_str), "%02X%02X", op1, op2);
        }
    } else if (op1 == 0xED || op1 == 0xCB) {
        snprintf(op_str, sizeof(op_str), "%02X%02X", op1, op2);
    } else {
        snprintf(op_str, sizeof(op_str), "%02X", op1);
    }

    // IM mode string (match Rust format)
    const char* im_str;
    switch (im) {
        case 0: im_str = "Mode0"; break;
        case 1: im_str = "Mode1"; break;
        case 2: im_str = "Mode2"; break;
        case 3: im_str = "Mode3"; break;
        default: im_str = "Mode0"; break;
    }

    fprintf(out, "%06llu %08llu %06X %06X %04X %06X %06X %06X %06X %06X %d %d %d %s %d %s\n",
            (unsigned long long)step,
            (unsigned long long)cycles,
            pc, sp, af, bc, de, hl, ix, iy,
            adl, iff1, iff2, im_str, halted, op_str);
}

void save_ppm(const uint32_t* fb, int w, int h, const char* filename) {
    FILE* f = fopen(filename, "wb");
    if (!f) return;
    fprintf(f, "P6\n%d %d\n255\n", w, h);
    for (int i = 0; i < w * h; i++) {
        uint32_t pixel = fb[i];
        fputc((pixel >> 16) & 0xFF, f);
        fputc((pixel >> 8) & 0xFF, f);
        fputc(pixel & 0xFF, f);
    }
    fclose(f);
}

int main(int argc, char* argv[]) {
    uint64_t max_steps = 1000000; // Default 1M steps
    const char* rom_path = NULL;
    const char* output_path = NULL;

    // Parse arguments
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "-n") == 0 && i + 1 < argc) {
            max_steps = strtoull(argv[++i], NULL, 10);
        } else if (strcmp(argv[i], "-o") == 0 && i + 1 < argc) {
            output_path = argv[++i];
        } else if (!rom_path) {
            rom_path = argv[i];
        }
    }

    if (!rom_path) {
        fprintf(stderr, "Usage: %s <rom_file> [-n steps] [-o output]\n", argv[0]);
        fprintf(stderr, "  -n steps   Number of steps to trace (default: 1000000)\n");
        fprintf(stderr, "  -o output  Output file (default: stdout)\n");
        return 1;
    }

    // Read ROM
    FILE* f = fopen(rom_path, "rb");
    if (!f) {
        fprintf(stderr, "Failed to open ROM: %s\n", rom_path);
        return 1;
    }

    fseek(f, 0, SEEK_END);
    size_t rom_size = ftell(f);
    rewind(f);

    uint8_t* rom_data = malloc(rom_size);
    if (!rom_data || fread(rom_data, 1, rom_size, f) != rom_size) {
        fprintf(stderr, "Failed to read ROM\n");
        fclose(f);
        return 1;
    }
    fclose(f);

    // Write ROM to temp file for CEmu
    const char* temp_path = "/tmp/cemu_trace_rom.rom";
    f = fopen(temp_path, "wb");
    if (!f || fwrite(rom_data, 1, rom_size, f) != rom_size) {
        fprintf(stderr, "Failed to write temp ROM\n");
        return 1;
    }
    fclose(f);
    free(rom_data);

    // Load ROM via CEmu
    emu_state_t state = emu_load(EMU_DATA_ROM, temp_path);
    if (state != EMU_STATE_VALID) {
        fprintf(stderr, "Failed to load ROM in CEmu\n");
        return 1;
    }
    emu_set_run_rate(48000000);

    // Open output
    FILE* out = stdout;
    if (output_path) {
        out = fopen(output_path, "w");
        if (!out) {
            fprintf(stderr, "Failed to open output: %s\n", output_path);
            return 1;
        }
    }

    fprintf(stderr, "=== CEmu Trace Generation (%llu steps) ===\n", (unsigned long long)max_steps);

    uint64_t step = 0;
    uint64_t total_base_ticks = 0;

    // Log initial state (step 0, before any instruction executes)
    log_trace_line(out, step, total_base_ticks);

    // Use emu_run with minimal tick increments to detect each instruction
    // At 48MHz, 160 base ticks = 1 CPU cycle
    // Run 1 tick at a time for finest granularity
    const uint64_t TICKS_PER_STEP = 1;

    while (step < max_steps) {
        uint32_t pc_before = cpu.registers.PC;
        bool halted_before = cpu.halted;

        // Run for 1 CPU cycle worth of base ticks
        emu_run(TICKS_PER_STEP);
        total_base_ticks += TICKS_PER_STEP;

        // Detect instruction boundary: PC changed, or halted state changed
        if (cpu.registers.PC != pc_before || cpu.halted != halted_before) {
            step++;
            // Log state after this instruction completes
            log_trace_line(out, step, total_base_ticks);

            if (step % 100000 == 0) {
                fprintf(stderr, "Progress: %llu steps (%.1f%%)\n",
                        (unsigned long long)step, 100.0 * step / max_steps);
            }

            // Log HALT transitions
            if (cpu.halted && !halted_before) {
                fprintf(stderr, "HALT at step %llu, PC=0x%06X\n",
                        (unsigned long long)step, cpu.registers.PC);
            }
        }
    }

    if (out != stdout) {
        fclose(out);
    }

    // Save final screenshot
    uint32_t framebuffer[LCD_WIDTH * LCD_HEIGHT];
    emu_lcd_drawframe(framebuffer);
    save_ppm(framebuffer, LCD_WIDTH, LCD_HEIGHT, "cemu_trace_final.ppm");

    fprintf(stderr, "\nTrace complete: %llu steps / %llu base ticks\n",
            (unsigned long long)step, (unsigned long long)total_base_ticks);
    fprintf(stderr, "Final PC: 0x%06X\n", cpu.registers.PC);
    if (output_path) {
        fprintf(stderr, "Saved to: %s\n", output_path);
    }
    fprintf(stderr, "Screenshot: cemu_trace_final.ppm\n");

    asic_free();
    return 0;
}
