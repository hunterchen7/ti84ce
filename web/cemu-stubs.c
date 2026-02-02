/**
 * GUI stubs for CEmu WASM build
 * These functions are called by the CEmu core but not needed for headless/web operation
 */

#include <stdint.h>
#include <stdbool.h>

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
