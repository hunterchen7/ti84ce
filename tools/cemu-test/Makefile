# CEmu Test Tools
#
# Test tools for comparing CEmu behavior with our Rust emulator.
#
# Prerequisites:
#   1. Clone CEmu into cemu-ref/:
#      git clone https://github.com/CE-Programming/CEmu.git cemu-ref
#
#   2. Build CEmu core library:
#      cd cemu-ref/core && make
#
#   3. Have a TI-84 Plus CE ROM file (not included, obtain legally)
#
# Build:
#   make              - Build all tools
#   make parity_check - Build parity checker only
#   make trace_gen    - Build trace generator only
#   make clean        - Remove built files
#
# Usage:
#   ./parity_check [rom_path] [-m max_cycles]
#   ./trace_gen <rom_path> [-n steps] [-o output]

# Path to CEmu (cloned at repo root)
CEMU_DIR = ../../cemu-ref

CC = gcc
CFLAGS = -Wall -Wextra -O2 -std=gnu11 -I$(CEMU_DIR)/core
LDFLAGS = -L$(CEMU_DIR)/core -lcemucore

# Main tools
TARGETS = trace_gen parity_check

all: check-cemu $(TARGETS)

# Verify CEmu is set up
check-cemu:
	@if [ ! -f $(CEMU_DIR)/core/libcemucore.a ]; then \
		echo "Error: CEmu core library not found."; \
		echo "Please run: cd $(CEMU_DIR)/core && make"; \
		exit 1; \
	fi

# CPU trace generator for parity comparison
# Usage: ./trace_gen <rom> [-n steps] [-o output]
trace_gen: trace_gen.c $(CEMU_DIR)/core/libcemucore.a
	$(CC) $(CFLAGS) $< -o $@ $(LDFLAGS)

# Parity check tool - RTC timing, MathPrint, state comparison
# Usage: ./parity_check [rom] [-v] [-m cycles]
parity_check: parity_check.c $(CEMU_DIR)/core/libcemucore.a
	$(CC) $(CFLAGS) $< -o $@ $(LDFLAGS)

# Wrapper library for external integration (JNI, FFI, etc.)
libcemu_wrapper.a: cemu_wrapper.o
	ar rcs $@ $^

cemu_wrapper.o: cemu_wrapper.c cemu_wrapper.h
	$(CC) $(CFLAGS) -c $< -o $@

# Legacy tools (kept for reference)
legacy: test_cemu test_wrapper

test_cemu: test_cemu.c $(CEMU_DIR)/core/libcemucore.a
	$(CC) $(CFLAGS) $< -o $@ $(LDFLAGS)

test_wrapper: test_wrapper.c cemu_wrapper.c cemu_wrapper.h $(CEMU_DIR)/core/libcemucore.a
	$(CC) $(CFLAGS) test_wrapper.c cemu_wrapper.c -o $@ $(LDFLAGS)

clean:
	rm -f $(TARGETS) test_cemu test_wrapper *.ppm *.o *.a

.PHONY: all clean legacy check-cemu
