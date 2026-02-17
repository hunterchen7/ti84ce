import { useEffect, useMemo, useRef, useState, useCallback } from "react";
import {
  createBackend,
  type EmulatorBackend,
  type BackendType,
} from "./emulator";
import { Keypad } from "./components/keypad";
import {
  BODY_ASPECT_RATIO,
  LCD_POSITION,
} from "./components/keypad/buttonRegions";
import { getStateStorage, type StateStorage } from "./storage/StateStorage";

// Lazy-load ROM module - not downloaded until needed
async function loadBundledRom(): Promise<Uint8Array | null> {
  try {
    const { decodeRom } = await import("./assets/rom");
    return await decodeRom();
  } catch {
    // ROM module failed to load
  }
  return null;
}

// TI-84 Plus CE keypad layout (matches iOS KeypadView.swift)
// Maps keyboard keys to [row, col] positions
const KEY_MAP: Record<string, [number, number]> = {
  // Function keys (row 1)
  F1: [1, 4], // Y=
  F2: [1, 3], // Window
  F3: [1, 2], // Zoom
  F4: [1, 1], // Trace
  F5: [1, 0], // Graph
  // Shift (2nd) handled specially - only triggers on release if no other key pressed
  Escape: [6, 6], // Clear
  Backspace: [1, 7], // Del
  Delete: [1, 7], // Del

  // Row 2: on, sto, ln, log, x², x⁻¹, math, alpha
  o: [2, 0], // ON
  O: [2, 0], // ON
  Insert: [2, 1], // Sto
  l: [2, 2], // Ln
  L: [2, 2], // Ln
  g: [2, 3], // Log
  G: [2, 3], // Log
  r: [2, 5], // x⁻¹
  R: [2, 5], // x⁻¹
  m: [2, 6], // Math
  M: [2, 6], // Math
  Alt: [2, 7], // Alpha

  // Row 3: 0, 1, 4, 7, comma, sin, apps, X,T,θ,n
  "0": [3, 0],
  "1": [3, 1],
  "4": [3, 2],
  "7": [3, 3],
  ",": [3, 4],
  s: [3, 5], // Sin
  S: [3, 5], // Sin
  Home: [3, 6], // Apps
  x: [3, 7], // X,T,θ,n
  X: [3, 7], // X,T,θ,n

  // Row 4: ., 2, 5, 8, (, cos, prgm, stat
  ".": [4, 0],
  "2": [4, 1],
  "5": [4, 2],
  "8": [4, 3],
  "(": [4, 4],
  c: [4, 5], // Cos
  C: [4, 5], // Cos
  p: [4, 6], // Prgm
  P: [4, 6], // Prgm
  PageDown: [4, 6], // Prgm
  End: [4, 7], // Stat

  // Row 5: (-), 3, 6, 9, ), tan, vars
  _: [5, 0], // (-)
  "3": [5, 1],
  "6": [5, 2],
  "9": [5, 3],
  ")": [5, 4],
  t: [5, 5], // Tan
  T: [5, 5], // Tan
  PageUp: [5, 6], // Vars

  // Row 6: enter, +, -, ×, ÷, ^, clear
  Enter: [6, 0],
  "+": [6, 1],
  "-": [6, 2],
  "*": [6, 3], // ×
  "/": [6, 4], // ÷
  "^": [6, 5],
  Clear: [6, 6],

  // Row 7: D-pad (down, left, right, up)
  ArrowDown: [7, 0],
  ArrowLeft: [7, 1],
  ArrowRight: [7, 2],
  ArrowUp: [7, 3],
};

interface CalculatorProps {
  className?: string;
  defaultBackend?: BackendType;
  useBundledRom?: boolean;
  fullscreen?: boolean;
}

export function Calculator({
  className,
  defaultBackend = "rust",
  useBundledRom = true,
  fullscreen = false,
}: CalculatorProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const backendRef = useRef<EmulatorBackend | null>(null);
  const animationRef = useRef<number>(0);
  const [backendType, setBackendType] = useState<BackendType>(defaultBackend);
  const [isRunning, setIsRunning] = useState(false);
  const [romLoaded, setRomLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [initialized, setInitialized] = useState(false);
  const [fps, setFps] = useState(0);
  const [backendName, setBackendName] = useState("");
  // Non-linear speed steps: 0.25-2.5 by 0.25, 3-10 by 0.5, 11-20 by 1
  const speedSteps = useMemo(() => {
    const steps: number[] = [];
    for (let s = 0.25; s <= 2.5; s += 0.25) steps.push(s);
    for (let s = 3; s <= 10; s += 0.5) steps.push(s);
    for (let s = 11; s <= 20; s += 1) steps.push(s);
    return steps;
  }, []);
  const [speedIndex, setSpeedIndex] = useState(3); // index 3 = 1x
  const speed = speedSteps[speedIndex];
  const [programFiles, setProgramFiles] = useState<string[]>([]); // Names of loaded .8xp/.8xv files
  const [isDragging, setIsDragging] = useState(false);
  const dragCounterRef = useRef(0);
  const lastFrameTime = useRef(0);
  const frameCount = useRef(0);
  const romDataRef = useRef<Uint8Array | null>(null);
  const programDataRef = useRef<{ name: string; data: Uint8Array }[]>([]); // Loaded program file data
  const programHandlesRef = useRef<FileSystemFileHandle[]>([]); // File handles for re-reading from disk
  const speedRef = useRef(1); // Ref for use in animation loop
  const turboUntilRef = useRef(0); // Timestamp: turbo-speed until this time (for fast boot after live send)
  const storageRef = useRef<StateStorage | null>(null);
  const romHashRef = useRef<string | null>(null);
  const backendTypeRef = useRef<BackendType>(defaultBackend);
  const programInputRef = useRef<HTMLInputElement>(null);

  // Keep backendTypeRef in sync
  useEffect(() => {
    backendTypeRef.current = backendType;
  }, [backendType]);

  // Save state helper
  const saveState = useCallback(async () => {
    const backend = backendRef.current;
    const storage = storageRef.current;
    const romHash = romHashRef.current;

    if (!backend || !storage || !romHash || !romLoaded) return;

    try {
      const t0 = performance.now();
      const stateData = backend.saveState();
      const t1 = performance.now();
      if (stateData) {
        await storage.saveState(romHash, stateData, backendTypeRef.current);
        const t2 = performance.now();
        console.log(
          `[State] snapshot: ${(t1 - t0).toFixed(1)}ms (${(stateData.length / 1024 / 1024).toFixed(1)}MB), ` +
          `IndexedDB write: ${(t2 - t1).toFixed(1)}ms, total: ${(t2 - t0).toFixed(1)}ms`
        );
      }
    } catch (err) {
      console.error("[State] Failed to save state:", err);
    }
  }, [romLoaded]);

  // Initialize backend
  useEffect(() => {
    let cancelled = false;
    const oldBackend = backendRef.current;
    const oldAnimation = animationRef.current;

    // Clean up old backend synchronously
    if (oldAnimation) {
      cancelAnimationFrame(oldAnimation);
      animationRef.current = 0;
    }
    if (oldBackend) {
      oldBackend.destroy();
    }
    backendRef.current = null;

    console.log("[Init] useEffect fired, oldBackend:", !!oldBackend);

    const initBackend = async () => {
      setInitialized(false);
      setRomLoaded(false);
      setIsRunning(false);
      setError(null);

      try {
        // Initialize storage
        const storage = await getStateStorage();
        storageRef.current = storage;

        const backend = createBackend(backendType);
        await backend.init();

        if (cancelled) {
          // Don't call destroy()/free() here — the WASM allocator may reuse
          // the freed pointer for the next WasmEmu, corrupting its borrow state.
          // The FinalizationRegistry will clean up the orphaned object safely.
          return;
        }

        backendRef.current = backend;
        setBackendName(backend.name);
        setInitialized(true);

        // Helper to load ROM into the backend with state restore
        const loadRomIntoBackend = async (data: Uint8Array) => {
          if (cancelled) return;

          const romHash = await storage.getRomHash(data);
          romHashRef.current = romHash;

          let currentBackend = backend;
          const result = await currentBackend.loadRom(data);
          if (cancelled) return;

          if (result === 0) {
            let stateRestored = false;
            try {
              const savedState = await storage.loadState(romHash, backendType);
              console.log("[State] savedState:", savedState ? `${savedState.length} bytes` : "none");
              if (savedState && currentBackend.loadState(savedState)) {
                stateRestored = true;
                console.log("[State] restored, lcdOn:", currentBackend.isLcdOn(),
                  "dump:", (currentBackend as any).dumpState?.());
              }
            } catch (e) {
              console.warn(
                "[State] Failed to restore state, clearing stale data:",
                e,
              );
              await storage.deleteState(romHash, backendType).catch(() => {});

              // Backend may be poisoned after a WASM panic — recreate it
              try {
                currentBackend.destroy();
                const freshBackend = createBackend(backendType);
                await freshBackend.init();
                if (cancelled) return;
                const reloadResult = await freshBackend.loadRom(data);
                if (reloadResult !== 0) {
                  if (!cancelled) setError(`Failed to reload ROM: error code ${reloadResult}`);
                  return;
                }
                currentBackend = freshBackend;
                backendRef.current = freshBackend;
                setBackendName(freshBackend.name);
              } catch (retryErr) {
                console.error("[State] Failed to recreate backend after state restore failure:", retryErr);
                if (!cancelled) setError(`Backend recovery failed: ${retryErr}`);
                return;
              }
            }
            if (!cancelled) {
              setRomLoaded(true);
              setIsRunning(true);
              // Auto power-on: press ON if fresh boot OR if state was restored but device is sleeping
              const needsPowerOn = !stateRestored
                ? !currentBackend.isLcdOn()  // Fresh boot: LCD not on yet
                : currentBackend.isDeviceOff();  // State restored but device was sleeping
              if (needsPowerOn) {
                currentBackend.setKey(2, 0, true);
                setTimeout(() => currentBackend.setKey(2, 0, false), 300);
              }
            }
          } else {
            if (!cancelled) {
              setError(
                `Failed to load ROM with ${currentBackend.name}: error code ${result}`,
              );
            }
          }
        };

        if (romDataRef.current) {
          await loadRomIntoBackend(romDataRef.current);
        } else if (useBundledRom) {
          const bundledData = await loadBundledRom();
          if (bundledData && !cancelled) {
            romDataRef.current = bundledData;
            await loadRomIntoBackend(bundledData);
          }
        }
      } catch (err) {
        if (!cancelled) {
          setError(`Failed to initialize ${backendType} backend: ${err}`);
        }
      }
    };

    initBackend();

    return () => {
      cancelled = true;
      if (animationRef.current) {
        cancelAnimationFrame(animationRef.current);
        animationRef.current = 0;
      }
      // Destroy backend on unmount/re-run — next effect invocation
      // will also destroy via oldBackend above, but this covers final unmount.
      if (backendRef.current) {
        backendRef.current.destroy();
        backendRef.current = null;
      }
    };
  }, [backendType, useBundledRom]);

  // Debounced save — triggers 500ms after the last keypress
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const debouncedSave = useCallback(() => {
    if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    saveTimerRef.current = setTimeout(() => {
      saveState();
      saveTimerRef.current = null;
    }, 500);
  }, [saveState]);

  // Auto-save on visibility change and page unload
  useEffect(() => {
    const handleVisibilityChange = () => {
      if (document.visibilityState === "hidden") {
        saveState();
      }
    };

    const handleBeforeUnload = () => {
      saveState();
    };

    const autoSaveInterval = setInterval(() => saveState(), 10_000);

    document.addEventListener("visibilitychange", handleVisibilityChange);
    window.addEventListener("beforeunload", handleBeforeUnload);

    return () => {
      document.removeEventListener("visibilitychange", handleVisibilityChange);
      window.removeEventListener("beforeunload", handleBeforeUnload);
      clearInterval(autoSaveInterval);
      if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    };
  }, [saveState]);


  // Handle ROM file loading
  const handleRomLoad = useCallback(async (file: File) => {
    const backend = backendRef.current;
    const storage = storageRef.current;
    if (!backend || !storage) return;

    try {
      const buffer = await file.arrayBuffer();
      const data = new Uint8Array(buffer);
      console.log(`[ROM] File read: ${data.length} bytes`);
      romDataRef.current = data; // Store for backend switching

      // Compute ROM hash for state persistence
      const romHash = await storage.getRomHash(data);
      romHashRef.current = romHash;
      console.log(`[ROM] Hash: ${romHash}`);

      // Clear all saved states to avoid stale format mismatches
      await storage.clearAllStates().catch(() => {});

      // Create a fresh backend instance to avoid poisoned WASM state
      console.log("[ROM] Creating fresh backend...");
      backend.destroy();
      const freshBackend = createBackend(backendTypeRef.current);
      await freshBackend.init();
      backendRef.current = freshBackend;
      console.log("[ROM] Fresh backend ready, calling loadRom...");

      const result = await freshBackend.loadRom(data);
      console.log(`[ROM] loadRom returned: ${result}`);

      if (result === 0) {
        console.log("ROM loaded successfully, waiting for ON key press...");

        setRomLoaded(true);
        setIsRunning(true); // Auto-start
        setError(null);
      } else {
        setError(`Failed to load ROM: error code ${result}`);
      }
    } catch (err) {
      console.error("[ROM] Error during load:", err);
      if (err instanceof Error) {
        console.error("[ROM] Stack:", err.stack);
      }
      setError(`Failed to read ROM file: ${err}`);
    }
  }, []);

  // Render frame to canvas
  const renderFrame = useCallback(() => {
    const backend = backendRef.current;
    const canvas = canvasRef.current;
    if (!backend || !canvas) return;

    try {
      const ctx = canvas.getContext("2d");
      if (!ctx) return;

      const width = backend.getFramebufferWidth();
      const height = backend.getFramebufferHeight();

      // Show black screen when LCD is off (sleeping or disabled)
      if (!backend.isLcdOn()) {
        ctx.fillStyle = "#000";
        ctx.fillRect(0, 0, width, height);
        return;
      }

      // Get framebuffer as RGBA
      const rgba = backend.getFramebufferRGBA();

      // Create ImageData and draw - copy into a new Uint8ClampedArray
      const clampedData = new Uint8ClampedArray(rgba.length);
      clampedData.set(rgba);
      const imageData = new ImageData(clampedData, width, height);
      ctx.putImageData(imageData, 0, 0);
    } catch (e) {
      // Backend was destroyed during render - safe to ignore
      console.warn("Render error (safe to ignore during backend switch):", e);
    }
  }, []);

  // Update speed ref when speed changes
  useEffect(() => {
    speedRef.current = speed;
  }, [speed]);

  // Main emulation loop
  useEffect(() => {
    if (!isRunning || !romLoaded || !backendRef.current) return;

    let slowFrameCount = 0;
    let totalFrames = 0;
    let timeAccumulator = 0;
    let lastLoopTime = 0;
    const TARGET_FRAME_MS = 1000 / 60; // 16.67ms per emulated frame

    const loop = (timestamp: number) => {
      const backend = backendRef.current;
      if (!backend) return;

      // Time-accumulator approach: track how much real time has passed
      // and run exactly that many emulated frames (scaled by speed).
      // This ensures precise 60fps regardless of display refresh rate.
      // Turbo mode: temporarily boost speed during boot after live file send
      const turbo = performance.now() < turboUntilRef.current;
      const effectiveSpeed = turbo ? 20 : speedRef.current;
      const maxFramesPerTick = turbo ? 30 : 4;

      if (lastLoopTime > 0) {
        let delta = timestamp - lastLoopTime;
        // Clamp delta to avoid spiral of death after tab suspension
        if (delta > 200) delta = TARGET_FRAME_MS;
        timeAccumulator += delta * effectiveSpeed;
      }
      lastLoopTime = timestamp;

      try {
        // Run one emulated frame per 16.67ms of accumulated time
        let framesThisTick = 0;
        while (timeAccumulator >= TARGET_FRAME_MS) {
          const t0 = performance.now();
          backend.runFrame();
          const elapsed = performance.now() - t0;
          totalFrames++;
          framesThisTick++;
          timeAccumulator -= TARGET_FRAME_MS;

          // Detect slow frames (>100ms means we're blocking the UI thread)
          if (elapsed > 100) {
            slowFrameCount++;
            if (slowFrameCount <= 5 || slowFrameCount % 50 === 0) {
              console.warn(
                `[EMU] Slow frame #${slowFrameCount}: ${elapsed.toFixed(0)}ms (frame ${totalFrames})`,
              );
            }
          } else {
            if (slowFrameCount > 0) {
              console.log(`[EMU] Recovered after ${slowFrameCount} slow frames`);
            }
            slowFrameCount = 0;
          }

          // Safety: cap frames per rAF to avoid blocking UI
          if (framesThisTick >= maxFramesPerTick) {
            timeAccumulator = 0;
            break;
          }
        }

        // Render if we ran at least one frame
        if (framesThisTick > 0) {
          renderFrame();
        }
      } catch (e) {
        // Backend was destroyed during frame - safe to ignore
        console.warn("Frame error (safe to ignore during backend switch):", e);
        return;
      }

      // Calculate FPS (count emulated frames rendered to screen)
      frameCount.current++;
      const now = performance.now();
      if (now - lastFrameTime.current >= 1000) {
        setFps(frameCount.current);
        frameCount.current = 0;
        lastFrameTime.current = now;
      }

      animationRef.current = requestAnimationFrame(loop);
    };

    lastFrameTime.current = performance.now();
    frameCount.current = 0;
    animationRef.current = requestAnimationFrame(loop);

    return () => {
      if (animationRef.current) {
        cancelAnimationFrame(animationRef.current);
      }
    };
  }, [isRunning, romLoaded, renderFrame]);

  // Keyboard event handling
  useEffect(() => {
    const backend = backendRef.current;
    if (!backend || !romLoaded) return;

    // Track if Shift was pressed alone (for 2nd key)
    let shiftPressedAlone = false;

    const handleKeyDown = (e: KeyboardEvent) => {
      // Control shortcuts
      if (e.key === " ") {
        e.preventDefault();
        setIsRunning((prev) => !prev);
        return;
      }

      // Track Shift for 2nd key - only trigger on release if pressed alone
      if (e.key === "Shift") {
        shiftPressedAlone = true;
        return;
      }

      // If any other key is pressed while Shift is held, it's not a solo Shift press
      if (e.shiftKey) {
        shiftPressedAlone = false;
      }

      // Special combo keys (2nd + key sequences)
      if (e.key === "v" || e.key === "V") {
        e.preventDefault();
        // Square root: 2nd + x²
        backend.setKey(1, 5, true); // 2nd down
        setTimeout(() => {
          backend.setKey(1, 5, false); // 2nd up
          backend.setKey(2, 4, true); // x² down
          setTimeout(() => backend.setKey(2, 4, false), 50); // x² up
        }, 50);
        return;
      }

      // Ctrl+R / Cmd+R: resend last program files (override browser refresh)
      if ((e.ctrlKey || e.metaKey) && e.key === 'r') {
        if (programHandlesRef.current.length > 0 || programDataRef.current.length > 0) {
          e.preventDefault();
          resendPrograms();
          return;
        }
      }

      // Don't intercept browser shortcuts (Ctrl/Cmd + key)
      if (e.ctrlKey || e.metaKey) {
        return;
      }

      const mapping = KEY_MAP[e.key];
      if (mapping) {
        e.preventDefault();
        backend.setKey(mapping[0], mapping[1], true);
      }
    };

    const handleKeyUp = (e: KeyboardEvent) => {
      // Handle Shift release - trigger 2nd only if it was pressed alone
      if (e.key === "Shift") {
        if (shiftPressedAlone) {
          // Tap 2nd key (press and release)
          backend.setKey(1, 5, true);
          setTimeout(() => backend.setKey(1, 5, false), 50);
        }
        shiftPressedAlone = false;
        return;
      }

      const mapping = KEY_MAP[e.key];
      if (mapping) {
        e.preventDefault();
        backend.setKey(mapping[0], mapping[1], false);
        debouncedSave();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("keyup", handleKeyUp);

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("keyup", handleKeyUp);
    };
  }, [romLoaded, backendType, debouncedSave]);

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) {
      handleRomLoad(file);
    }
  };

  const handleReset = () => {
    if (backendRef.current) {
      backendRef.current.reset();
    }
  };

  const handleEjectRom = async () => {
    // Save state before ejecting
    await saveState();

    setIsRunning(false);
    setRomLoaded(false);
    romDataRef.current = null;
    romHashRef.current = null;
    if (backendRef.current) {
      backendRef.current.reset();
    }
  };

  // Core logic for injecting .8xp/.8xv program files
  const loadProgramFiles = useCallback(async (files: File[]) => {
    if (files.length === 0) return;

    const romData = romDataRef.current;
    if (!romData) {
      setError("Load a ROM first before sending program files");
      return;
    }

    // Read all selected files
    const fileEntries: { name: string; data: Uint8Array }[] = [];
    for (const file of files) {
      const buffer = await file.arrayBuffer();
      fileEntries.push({ name: file.name, data: new Uint8Array(buffer) });
    }

    // Store for future resets
    programDataRef.current = fileEntries;
    setProgramFiles(fileEntries.map(f => f.name));

    const backend = backendRef.current;

    // Live path: emulator is already running — inject + soft reboot in-place
    if (backend?.isRomLoaded) {
      try {
        let totalInjected = 0;
        for (const entry of fileEntries) {
          const count = backend.sendFileLive(entry.data);
          if (count >= 0) {
            totalInjected += count;
            console.log(`[Program Live] Injected ${entry.name}: ${count} entries`);
          } else {
            console.error(`[Program Live] Failed to inject ${entry.name}: error ${count}`);
          }
        }
        console.log(`[Program Live] Total entries injected: ${totalInjected}, soft reboot done`);
        // Turbo-speed through boot — slightly undershot so user doesn't notice
        turboUntilRef.current = performance.now() + 300;
        setError(null);
      } catch (err) {
        console.error("[Program Live] Error:", err);
        setError(`Failed to live-send programs: ${err}`);
      }
      return;
    }

    // Cold boot path: no emulator running — create fresh backend
    setIsRunning(false);
    setRomLoaded(false);
    if (animationRef.current) {
      cancelAnimationFrame(animationRef.current);
      animationRef.current = 0;
    }

    try {
      // Create fresh backend, load ROM, inject files, then boot
      const oldBackend = backendRef.current;
      if (oldBackend) oldBackend.destroy();

      const freshBackend = createBackend(backendTypeRef.current);
      await freshBackend.init();
      backendRef.current = freshBackend;

      const result = await freshBackend.loadRom(romData);
      if (result !== 0) {
        setError(`Failed to reload ROM: error code ${result}`);
        return;
      }

      // Inject each program file
      let totalInjected = 0;
      for (const entry of fileEntries) {
        const count = freshBackend.sendFile(entry.data);
        if (count >= 0) {
          totalInjected += count;
          console.log(`[Program] Injected ${entry.name}: ${count} entries`);
        } else {
          console.error(`[Program] Failed to inject ${entry.name}: error ${count}`);
        }
      }

      console.log(`[Program] Total entries injected: ${totalInjected}`);

      // Start animation loop, then press ON key to power on.
      // We use setKey instead of powerOn() because powerOn() does press+release
      // with zero cycles in between, leaving a stale irq_pending that causes
      // a spurious interrupt during boot. setKey + delayed release matches
      // the normal user flow (hold ON, boot runs, release ON).
      setRomLoaded(true);
      setIsRunning(true);
      setError(null);
      freshBackend.setKey(2, 0, true);  // ON key press → powered_on = true
      setTimeout(() => freshBackend.setKey(2, 0, false), 300);  // Release after boot starts
    } catch (err) {
      console.error("[Program] Error:", err);
      setError(`Failed to load programs: ${err}`);
    }
  }, []);

  // Resend last program files — re-reads from disk via FileSystemFileHandle if available
  const resendPrograms = useCallback(async () => {
    const backend = backendRef.current;
    if (!backend?.isRomLoaded) return;

    const handles = programHandlesRef.current;
    if (handles.length > 0) {
      // Re-read fresh bytes from disk
      try {
        let totalInjected = 0;
        for (const handle of handles) {
          const file = await handle.getFile();
          const data = new Uint8Array(await file.arrayBuffer());
          const count = backend.sendFileLive(data);
          if (count >= 0) {
            totalInjected += count;
            console.log(`[Resend] Injected ${file.name}: ${count} entries`);
          } else {
            console.error(`[Resend] Failed to inject ${file.name}: error ${count}`);
          }
        }
        console.log(`[Resend] Total entries injected: ${totalInjected}, soft reboot done`);
        turboUntilRef.current = performance.now() + 300;
        setError(null);
      } catch (err) {
        console.error("[Resend] Error:", err);
        setError(`Failed to resend programs: ${err}`);
      }
      return;
    }

    // Fallback: resend cached data
    const cached = programDataRef.current;
    if (cached.length > 0) {
      try {
        let totalInjected = 0;
        for (const entry of cached) {
          const count = backend.sendFileLive(entry.data);
          if (count >= 0) {
            totalInjected += count;
            console.log(`[Resend] Injected ${entry.name}: ${count} entries (cached)`);
          } else {
            console.error(`[Resend] Failed to inject ${entry.name}: error ${count}`);
          }
        }
        console.log(`[Resend] Total entries injected: ${totalInjected}, soft reboot done`);
        turboUntilRef.current = performance.now() + 300;
        setError(null);
      } catch (err) {
        console.error("[Resend] Error:", err);
        setError(`Failed to resend programs: ${err}`);
      }
    }
  }, []);

  // Handle file input change for .8xp/.8xv programs
  const handleProgramFiles = useCallback(async (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files;
    if (!files || files.length === 0) return;
    await loadProgramFiles(Array.from(files));
    // Reset the file input so the same files can be re-selected
    e.target.value = "";
  }, [loadProgramFiles]);

  // Drag-and-drop support for .rom, .8xp, .8xv files
  const PROGRAM_EXTENSIONS = [".8xp", ".8xv"];
  const ROM_EXTENSIONS = [".rom", ".bin"];

  const handleDragEnter = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragCounterRef.current++;
    if (dragCounterRef.current === 1) {
      setIsDragging(true);
    }
  }, []);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragCounterRef.current--;
    if (dragCounterRef.current === 0) {
      setIsDragging(false);
    }
  }, []);

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
  }, []);

  const handleDrop = useCallback(async (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragCounterRef.current = 0;
    setIsDragging(false);

    const files = Array.from(e.dataTransfer.files);
    if (files.length === 0) return;

    const romFiles: File[] = [];
    const programFilesList: File[] = [];

    // Try to capture FileSystemFileHandles for program files (Chromium only)
    const handles: FileSystemFileHandle[] = [];
    const items = Array.from(e.dataTransfer.items);

    for (let i = 0; i < files.length; i++) {
      const file = files[i];
      const name = file.name.toLowerCase();
      if (ROM_EXTENSIONS.some(ext => name.endsWith(ext))) {
        romFiles.push(file);
      } else if (PROGRAM_EXTENSIONS.some(ext => name.endsWith(ext))) {
        programFilesList.push(file);
        // Try to get a persistent handle for re-reading later
        const item = items[i];
        if (item && 'getAsFileSystemHandle' in item) {
          try {
            const handle = await (item as any).getAsFileSystemHandle();
            if (handle?.kind === 'file') handles.push(handle);
          } catch { /* Not supported or permission denied */ }
        }
      }
    }

    // Store handles if we got them for all program files
    if (handles.length === programFilesList.length && handles.length > 0) {
      programHandlesRef.current = handles;
    }

    // Load ROM first if present (only use the first one)
    if (romFiles.length > 0) {
      await handleRomLoad(romFiles[0]);
    }

    // Then inject program files
    if (programFilesList.length > 0) {
      await loadProgramFiles(programFilesList);
    }

    if (romFiles.length === 0 && programFilesList.length === 0) {
      setError("Unsupported file type. Drop .rom, .8xp, or .8xv files.");
    }
  }, [handleRomLoad, loadProgramFiles]);

  const handleBackendChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const newBackend = e.target.value as BackendType;
    setBackendType(newBackend);
  };

  // Keypad handlers
  const handleKeypadDown = useCallback((row: number, col: number) => {
    if (backendRef.current) {
      backendRef.current.setKey(row, col, true);
    }
  }, []);

  const handleKeypadUp = useCallback((row: number, col: number) => {
    if (backendRef.current) {
      backendRef.current.setKey(row, col, false);
      debouncedSave();
    }
  }, [debouncedSave]);

  // Calculate container width based on fullscreen mode
  const containerWidth = fullscreen
    ? "min(420px, 95vw, calc(95vh * 0.45))"
    : "360px";

  return (
    <div
      className={className}
      onDragEnter={handleDragEnter}
      onDragLeave={handleDragLeave}
      onDragOver={handleDragOver}
      onDrop={handleDrop}
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        gap: fullscreen ? "0.5rem" : "1rem",
        position: "relative",
        minHeight: "200px",
        ...(fullscreen && {
          transform: "scale(1)",
          transformOrigin: "center center",
        }),
      }}
    >
      {/* Drag-and-drop overlay */}
      {isDragging && (
        <div
          style={{
            position: "absolute",
            inset: 0,
            background: "rgba(59, 130, 246, 0.15)",
            border: "3px dashed rgba(59, 130, 246, 0.6)",
            borderRadius: "12px",
            zIndex: 100,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            pointerEvents: "none",
          }}
        >
          <span style={{
            fontSize: "1.25rem",
            fontWeight: "bold",
            color: "rgba(59, 130, 246, 0.8)",
            background: "rgba(255, 255, 255, 0.9)",
            padding: "0.75rem 1.5rem",
            borderRadius: "8px",
          }}>
            Drop .rom, .8xp, or .8xv files
          </span>
        </div>
      )}
      {/* Header and backend selector - only on non-demo */}
      {!useBundledRom && (
        <>
          <h1>TI-84 Plus CE Emulator</h1>

          <div style={{ display: "flex", gap: "1rem", alignItems: "center" }}>
            <label htmlFor="backend-select">Backend:</label>
            <select
              id="backend-select"
              value={backendType}
              onChange={handleBackendChange}
              disabled={isRunning}
              style={{ padding: "0.5rem" }}
            >
              <option value="rust">Rust (Custom)</option>
              <option value="cemu">CEmu (Reference)</option>
            </select>
            {backendName && (
              <span style={{ fontSize: "0.875rem", color: "#666" }}>
                Using: {backendName}
              </span>
            )}
          </div>
        </>
      )}

      {error && (
        <div
          style={{
            color: "red",
            padding: "0.5rem",
            background: "#fee",
            borderRadius: "4px",
          }}
        >
          {error}
        </div>
      )}

      {!initialized && !useBundledRom && (
        <p>
          Loading {backendType === "rust" ? "Rust WASM" : "CEmu WASM"} module...
        </p>
      )}

      {initialized && !romLoaded && !useBundledRom && (
        <div
          style={{
            padding: "1rem",
            border: "2px dashed #ccc",
            borderRadius: "8px",
          }}
        >
          <label htmlFor="rom-input" style={{ cursor: "pointer" }}>
            <p>Select a TI-84 Plus CE ROM file (.rom)</p>
            <input
              id="rom-input"
              type="file"
              accept=".rom,.bin"
              onChange={handleFileChange}
              style={{ marginTop: "0.5rem" }}
            />
          </label>
        </div>
      )}

      {(romLoaded || useBundledRom) && (
        <>
          {/* Calculator — single combined image (bezel + keypad) */}
          <div
            style={{
              boxShadow: "0 8px 24px rgba(0,0,0,0.4)",
              width: containerWidth,
              borderBottomLeftRadius: "48px",
              borderBottomRightRadius: "48px",
              borderTopLeftRadius: 56,
              borderTopRightRadius: 56,
              overflow: "hidden",
              position: "relative",
              aspectRatio: `${BODY_ASPECT_RATIO}`,
              backgroundImage: "url(/buttons/calculator_body.png)",
              backgroundSize: "100% 100%",
            }}
          >
            {/* LCD canvas positioned within combined body */}
            <canvas
              ref={canvasRef}
              width={320}
              height={240}
              style={{
                position: "absolute",
                left: `${LCD_POSITION.left}%`,
                top: `${LCD_POSITION.top}%`,
                width: `${LCD_POSITION.width}%`,
                height: `${LCD_POSITION.height}%`,
                imageRendering: "pixelated",
              }}
            />
            {!romLoaded && useBundledRom && (
              <div
                style={{
                  position: "absolute",
                  left: `${LCD_POSITION.left}%`,
                  top: `${LCD_POSITION.top}%`,
                  width: `${LCD_POSITION.width}%`,
                  height: `${LCD_POSITION.height}%`,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  background: "#000",
                  color: "#888",
                  fontSize: "1rem",
                  fontFamily: "system-ui, sans-serif",
                }}
              >
                Loading...
              </div>
            )}

            {/* Keypad buttons overlay (positioned over keypad portion) */}
            <Keypad onKeyDown={handleKeypadDown} onKeyUp={handleKeypadUp} />
          </div>

          {/* Controls - outside calculator */}
          <div
            style={{
              display: "flex",
              gap: "0.75rem",
              alignItems: "center",
              flexWrap: "wrap",
              justifyContent: "center",
            }}
          >
            <button
              onClick={() => setIsRunning(!isRunning)}
              style={{ padding: "6px 16px" }}
              title="Space"
            >
              {isRunning ? "Pause" : "Run"}
            </button>
            <button
              onClick={handleReset}
              style={{ padding: "6px 16px" }}
              title="R"
            >
              Reset
            </button>
            {!useBundledRom && (
              <>
                <button
                  onClick={handleEjectRom}
                  style={{ padding: "6px 16px" }}
                  title="E"
                >
                  Eject
                </button>
                <button
                  onClick={() => programInputRef.current?.click()}
                  style={{ padding: "6px 16px" }}
                  title="Load .8xp/.8xv programs"
                >
                  Send File
                </button>
                <input
                  ref={programInputRef}
                  type="file"
                  accept=".8xp,.8xv"
                  multiple
                  onChange={handleProgramFiles}
                  style={{ display: "none" }}
                />
                {programFiles.length > 0 && (
                  <>
                    <button
                      onClick={resendPrograms}
                      style={{ padding: "6px 16px" }}
                      title="Re-read and resend last program files (Ctrl+R)"
                    >
                      Resend
                    </button>
                    <span style={{ fontSize: "0.75rem", color: "#888" }}>
                      {programFiles.join(", ")}
                    </span>
                  </>
                )}
              </>
            )}
            <div
              style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}
            >
              <input
                type="range"
                min="0"
                max={speedSteps.length - 1}
                step="1"
                value={speedIndex}
                onChange={(e) => setSpeedIndex(parseInt(e.target.value))}
                style={{ width: "80px" }}
                title="CPU Speed"
              />
              <span
                style={{
                  fontSize: "0.75rem",
                  color: fullscreen ? "#888" : "#666",
                  minWidth: "2.5rem",
                }}
              >
                {speed}x
              </span>
            </div>
            <span
              style={{
                fontSize: "0.875rem",
                color: fullscreen ? "#888" : "#666",
              }}
            >
              {fps} FPS
            </span>
          </div>

          {/* Keyboard controls help */}
          <div
            style={{
              fontSize: "0.75rem",
              color: "#888",
              maxWidth: "360px",
              textAlign: "center",
            }}
          >
            <p>
              <strong>Keyboard Controls:</strong>
            </p>
            <p>
              Numbers: 0-9 | Arrows: Navigate | Enter: Enter | Backspace: Del
            </p>
            <p>+, -, *, / : Math | ( ) : Parens | ^: Power | V: √</p>
            <p>Shift: 2nd | Alt: Alpha | Escape: Clear | O: ON | P: Prgm | Space: Pause</p>
          </div>
        </>
      )}
    </div>
  );
}
