/*
 * CEmu Parity Check Tool
 *
 * Comprehensive tool for comparing CEmu state with our Rust emulator.
 * Checks RTC timing, MathPrint flag, and other key state at cycle milestones.
 *
 * Usage: ./parity_check [rom_path] [-v] [-m cycles]
 *   rom_path   Path to TI-84 CE ROM (default: ../../TI-84 CE.rom)
 *   -v         Verbose mode (more detailed output)
 *   -m cycles  Maximum cycles to run (default: 60M)
 *
 * Key addresses monitored:
 *   0xD000C4 - MathPrint flag (bit 5: 1=MathPrint, 0=Classic)
 *   0xF80020 - RTC control register (bit 6: load in progress)
 *   0xF80040 - RTC load status (0x00=complete, 0xF8=all pending)
 *
 * Expected behavior:
 *   - RTC load stays pending (0xF8) until ~24M cycles at 48MHz
 *   - MathPrint flag should be set (0x20) after boot completes
 */

#include "../../cemu-ref/core/emu.h"
#include "../../cemu-ref/core/asic.h"
#include "../../cemu-ref/core/mem.h"
#include "../../cemu-ref/core/cpu.h"
#include "../../cemu-ref/core/realclock.h"
#include "../../cemu-ref/core/lcd.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdbool.h>

/* GUI stubs required by CEmu */
void gui_console_clear(void) {}
void gui_console_printf(const char *format, ...) { (void)format; }
void gui_console_err_printf(const char *format, ...) { (void)format; }
asic_rev_t gui_handle_reset(const boot_ver_t* b, asic_rev_t l, asic_rev_t d,
                             emu_device_t e, bool* p) {
    (void)b; (void)e; (void)p;
    return (l != ASIC_REV_AUTO) ? l : d;
}
#ifdef DEBUG_SUPPORT
void gui_debug_open(int r, uint32_t d) { (void)r; (void)d; }
void gui_debug_close(void) {}
#endif

/* Key memory addresses */
#define MATHPRINT_ADDR  0xD000C4
#define RTC_CONTROL     0xF80020
#define RTC_LOAD_STATUS 0xF80040

/* Default cycle milestones to check */
static uint32_t default_milestones[] = {
    1000000,   /*  1M - Very early boot */
    5000000,   /*  5M - Early boot */
    10000000,  /* 10M - Boot progress */
    20000000,  /* 20M - Before first RTC load trigger */
    25000000,  /* 25M - First load should be pending */
    26000000,  /* 26M - Fine granularity */
    27000000,  /* 27M - Poll loop region */
    27500000,  /* 27.5M - Where we found 0xF8 status */
    28000000,  /* 28M - Fine granularity */
    29000000,  /* 29M - Load may complete here */
    30000000,  /* 30M - After initial load */
    40000000,  /* 40M - Mid boot */
    50000000,  /* 50M - Late boot */
    60000000,  /* 60M - Should be near home screen */
};
#define NUM_DEFAULT_MILESTONES (sizeof(default_milestones) / sizeof(default_milestones[0]))

void print_header(void) {
    printf("=== CEmu Parity Check ===\n\n");
    printf("Cycle(M)  | RTC Ctrl | RTC Status | loadTicks | mode | MathPrint | PC\n");
    printf("----------|----------|------------|-----------|------|-----------|--------\n");
}

void print_state(uint32_t cycle_millions) {
    /* Read RTC state directly from struct (more reliable than mem_peek) */
    uint8_t rtc_ctrl = rtc.control;
    int8_t ticks = rtc.loadTicksProcessed;

    /* Compute load status same way as rtc_read() does for offset 0x40 */
    uint8_t rtc_status;
    if (ticks >= 51) {  /* LOAD_TOTAL_TICKS */
        rtc_status = 0x00;  /* Load complete */
    } else {
        /* Bits set indicate load still in progress for each field */
        rtc_status = 8 | ((ticks < 9) ? 0x10 : 0)    /* sec */
                       | ((ticks < 17) ? 0x20 : 0)   /* min */
                       | ((ticks < 25) ? 0x40 : 0)   /* hour */
                       | ((ticks < 41) ? 0x80 : 0);  /* day */
    }

    uint8_t mathprint = mem_peek_byte(MATHPRINT_ADDR);
    const char* mp_str = (mathprint & 0x20) ? "MathPrint" : "Classic  ";

    printf("%9u | 0x%02X     | 0x%02X       | %9d | %4d | 0x%02X %s | 0x%06X\n",
           cycle_millions,
           rtc_ctrl,
           rtc_status,
           rtc.loadTicksProcessed,
           rtc.mode,
           mathprint,
           mp_str,
           cpu.registers.PC);
}

void print_summary(void) {
    uint8_t mathprint = mem_peek_byte(MATHPRINT_ADDR);

    printf("\n=== Summary ===\n");
    printf("Final MathPrint byte: 0x%02X\n", mathprint);
    printf("MathPrint mode: %s\n", (mathprint & 0x20) ? "ENABLED (MathPrint)" : "DISABLED (Classic)");
    printf("Final PC: 0x%06X\n", cpu.registers.PC);
    printf("Total cycles: %llu\n", (unsigned long long)cpu.cycles);

    /* Check expected state */
    printf("\n=== Parity Checks ===\n");
    if (mathprint & 0x20) {
        printf("[PASS] MathPrint flag is set\n");
    } else {
        printf("[FAIL] MathPrint flag is NOT set (expected MathPrint mode)\n");
    }
}

void save_screenshot(const char* filename) {
    uint32_t fb[320 * 240];
    emu_lcd_drawframe(fb);

    FILE* f = fopen(filename, "wb");
    if (!f) return;

    fprintf(f, "P6\n320 240\n255\n");
    for (int i = 0; i < 320 * 240; i++) {
        fputc((fb[i] >> 16) & 0xFF, f);
        fputc((fb[i] >> 8) & 0xFF, f);
        fputc(fb[i] & 0xFF, f);
    }
    fclose(f);
    printf("Screenshot saved: %s\n", filename);
}

int main(int argc, char* argv[]) {
    const char* rom_path = "../../TI-84 CE.rom";
    bool verbose = false;
    uint64_t max_cycles = 60000000;

    /* Parse arguments */
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "-v") == 0) {
            verbose = true;
        } else if (strcmp(argv[i], "-m") == 0 && i + 1 < argc) {
            max_cycles = strtoull(argv[++i], NULL, 10);
        } else if (argv[i][0] != '-') {
            rom_path = argv[i];
        }
    }

    /* Load ROM */
    FILE* f = fopen(rom_path, "rb");
    if (!f) {
        fprintf(stderr, "ROM not found: %s\n", rom_path);
        return 1;
    }

    fseek(f, 0, SEEK_END);
    size_t sz = ftell(f);
    rewind(f);

    uint8_t* rom = malloc(sz);
    fread(rom, 1, sz, f);
    fclose(f);

    /* Write to temp file for CEmu */
    f = fopen("/tmp/parity_check.rom", "wb");
    fwrite(rom, 1, sz, f);
    fclose(f);
    free(rom);

    if (emu_load(EMU_DATA_ROM, "/tmp/parity_check.rom") != EMU_STATE_VALID) {
        fprintf(stderr, "Failed to load ROM\n");
        return 1;
    }

    emu_set_run_rate(48000000);

    print_header();

    /* Run to each milestone */
    size_t milestone_idx = 0;
    while (milestone_idx < NUM_DEFAULT_MILESTONES &&
           default_milestones[milestone_idx] <= max_cycles) {

        uint32_t target = default_milestones[milestone_idx];

        while (cpu.cycles < target) {
            emu_run(100000);
        }

        print_state(target / 1000000);
        milestone_idx++;
    }

    print_summary();
    save_screenshot("parity_check_final.ppm");

    asic_free();
    return 0;
}
