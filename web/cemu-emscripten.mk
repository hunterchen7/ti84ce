# CEmu WASM build with modern Emscripten flags
# Based on cemu-ref/core/emscripten.mk but updated for current Emscripten

CC      = emcc

# Add -g3 and disable some opts if needed
CFLAGS  = -W -Wall -O3 -flto

# For console printing, software commands etc.
CFLAGS += -DDEBUG_SUPPORT

# Modern Emscripten flags (updated from deprecated ones)
EMFLAGS := -s TOTAL_MEMORY=33554432 -s WASM=1 -s EXPORT_ES6=1 -s MODULARIZE=1 -s EXPORT_NAME="'WebCEmu'" -s INVOKE_RUN=0 -s NO_EXIT_RUNTIME=1 -s ASSERTIONS=0 -s "EXPORTED_RUNTIME_METHODS=['FS', 'callMain', 'ccall', 'cwrap']"

LFLAGS := -flto $(EMFLAGS)

CEMU_CORE := ../cemu-ref/core

CSOURCES := $(wildcard $(CEMU_CORE)/*.c) $(wildcard $(CEMU_CORE)/usb/*.c) $(CEMU_CORE)/debug/debug.c $(CEMU_CORE)/os/os-emscripten.c

OBJS = $(patsubst $(CEMU_CORE)/%.c, build-cemu/%.bc, $(CSOURCES))

# Local stubs for missing GUI functions
STUB_OBJS = build-cemu/cemu-stubs.bc

OUTPUT := build-cemu/WebCEmu

.PHONY: wasm all clean dirs

wasm: dirs $(OUTPUT).js
	@echo "CEmu WASM built successfully!"

all: wasm

dirs:
	@mkdir -p build-cemu/usb build-cemu/debug build-cemu/os

build-cemu/%.bc: $(CEMU_CORE)/%.c
	$(CC) $(CFLAGS) -I$(CEMU_CORE) -c $< -o $@

build-cemu/cemu-stubs.bc: cemu-stubs.c
	$(CC) $(CFLAGS) -I$(CEMU_CORE) -c $< -o $@

$(OUTPUT).js: $(OBJS) $(STUB_OBJS)
	$(CC) $(CFLAGS) $(LFLAGS) $^ -o $@

clean:
	rm -rf build-cemu
