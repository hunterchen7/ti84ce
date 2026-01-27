# Architecture

## Overview

The TI-84 Plus CE emulator is designed with a clean separation between the platform-agnostic emulator core and platform-specific shells.

```
┌─────────────────────────────────────────────────────────┐
│                    Platform Shell                        │
│  (Android: Kotlin/Compose, iOS: Swift/SwiftUI)          │
├─────────────────────────────────────────────────────────┤
│                    C ABI Bridge                          │
│  (Android: JNI/C++, iOS: Swift C interop)               │
├─────────────────────────────────────────────────────────┤
│                    Rust Core                             │
│  (Platform-agnostic, no OS APIs)                        │
└─────────────────────────────────────────────────────────┘
```

## Core Design Principles

### 1. No Platform Dependencies
The core contains no file I/O, no platform logging, and no threading. All data is passed in/out via byte buffers.

### 2. Stable C ABI
The core exposes a stable C ABI that doesn't leak Rust types. This allows any platform to integrate with minimal effort.

### 3. Single-Threaded Core
The core is single-threaded and deterministic. Threading is owned by the platform shell.

### 4. Buffer-Based I/O
- ROM is passed in as bytes
- Framebuffer is exposed as a pointer to ARGB8888 data
- Save states are serialized to/from byte buffers

## Core Components

### Emu (Orchestrator)
The main emulator struct that owns all subsystems and coordinates execution.

### CPU (eZ80)
The eZ80 processor implementation with:
- Registers and flags
- Instruction decoder and executor
- Cycle counting

### Bus
Memory bus with address decoding for:
- RAM
- Flash (ROM)
- MMIO regions

### Memory
RAM and Flash backing stores with:
- Memory paging
- Bank switching

### Hardware
Peripheral implementations:
- LCD controller
- Keypad matrix
- Timers
- Interrupt controller

## Android Shell

### JNI Bridge
Thin C++ layer that:
- Holds `Emu*` as `jlong` handle
- Converts between Java types and C types
- Copies framebuffer data

### Compose UI
- Single activity with Compose-based UI
- Emulation runs on background coroutine
- Framebuffer rendered as Bitmap
- Touch events mapped to keypad matrix

## Data Flow

### ROM Loading
```
User picks file → ContentResolver reads bytes → JNI passes to core → Core stores in flash
```

### Frame Rendering
```
Core updates framebuffer → JNI copies to IntArray → Kotlin creates Bitmap → Compose renders
```

### Key Input
```
Touch event → Compose callback → JNI call → Core updates key matrix
```
