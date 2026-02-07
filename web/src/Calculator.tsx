import { useEffect, useRef, useState, useCallback } from "react";
import {
  createBackend,
  type EmulatorBackend,
  type BackendType,
} from "./emulator";
import { Keypad } from "./components/keypad";
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
  Escape: [1, 6], // Mode
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
  const [speed, setSpeed] = useState(1); // Speed multiplier (0.25x to 4x)
  const lastFrameTime = useRef(0);
  const frameCount = useRef(0);
  const romDataRef = useRef<Uint8Array | null>(null);
  const speedRef = useRef(1); // Ref for use in animation loop
  const storageRef = useRef<StateStorage | null>(null);
  const romHashRef = useRef<string | null>(null);
  const backendTypeRef = useRef<BackendType>(defaultBackend);

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
      const stateData = backend.saveState();
      if (stateData) {
        await storage.saveState(romHash, stateData, backendTypeRef.current);
        console.log('[State] Saved state for ROM:', romHash, 'backend:', backendTypeRef.current);
      }
    } catch (err) {
      console.error('[State] Failed to save state:', err);
    }
  }, [romLoaded]);

  // Initialize backend
  useEffect(() => {
    let cancelled = false;
    const oldBackend = backendRef.current;
    const oldAnimation = animationRef.current;

    // Clean up old backend synchronously first
    if (oldAnimation) {
      cancelAnimationFrame(oldAnimation);
      animationRef.current = 0;
    }

    // Clear the ref before destroying to prevent any in-flight calls
    backendRef.current = null;

    // Small delay to let any pending operations complete
    const cleanup = () => {
      if (oldBackend) {
        oldBackend.destroy();
      }
    };

    // Defer cleanup to next tick to avoid race conditions
    setTimeout(cleanup, 0);

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
          // Component unmounted or backend changed during init
          backend.destroy();
          return;
        }

        backendRef.current = backend;
        setBackendName(backend.name);
        setInitialized(true);

        // If we had a ROM loaded, reload it
        if (romDataRef.current) {
          const romHash = await storage.getRomHash(romDataRef.current);
          romHashRef.current = romHash;

          const result = await backend.loadRom(romDataRef.current);
          if (result === 0) {
            // Try to load saved state (namespaced by backend)
            let restored = false;
            try {
              const savedState = await storage.loadState(romHash, backendType);
              if (savedState && backend.loadState(savedState)) {
                console.log('[State] Restored state for ROM:', romHash, 'backend:', backendType);
                restored = true;
              }
            } catch (e) {
              console.warn('[State] Failed to restore state, clearing stale data:', e);
              await storage.deleteState(romHash, backendType).catch(() => {});
            }
            if (!restored) {
              backend.powerOn();
            }
            setRomLoaded(true);
            setIsRunning(true); // Auto-start after backend switch
          } else {
            setError(
              `Failed to load ROM with ${backend.name}: error code ${result}`,
            );
          }
        } else if (useBundledRom) {
          // Defer ROM loading to let UI render first
          setTimeout(async () => {
            if (cancelled) return;
            const bundledData = await loadBundledRom();
            if (bundledData) {
              romDataRef.current = bundledData;
              const romHash = await storage.getRomHash(bundledData);
              romHashRef.current = romHash;

              const result = await backend.loadRom(bundledData);
              if (result === 0) {
                // Try to load saved state (namespaced by backend)
                let restored = false;
                try {
                  const savedState = await storage.loadState(romHash, backendType);
                  if (savedState && backend.loadState(savedState)) {
                    console.log('[State] Restored state for ROM:', romHash, 'backend:', backendType);
                    restored = true;
                  }
                } catch (e) {
                  console.warn('[State] Failed to restore state, clearing stale data:', e);
                  await storage.deleteState(romHash, backendType).catch(() => {});
                }
                if (!restored) {
                  backend.powerOn();
                }
                setRomLoaded(true);
                setIsRunning(true);
              }
            }
          }, 0);
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
      // Don't destroy here - will be handled on next init or unmount
    };
  }, [backendType, useBundledRom]);

  // Auto-save on visibility change and page unload
  useEffect(() => {
    const handleVisibilityChange = () => {
      if (document.visibilityState === 'hidden') {
        saveState();
      }
    };

    const handleBeforeUnload = () => {
      saveState();
    };

    document.addEventListener('visibilitychange', handleVisibilityChange);
    window.addEventListener('beforeunload', handleBeforeUnload);

    return () => {
      document.removeEventListener('visibilitychange', handleVisibilityChange);
      window.removeEventListener('beforeunload', handleBeforeUnload);
    };
  }, [saveState]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (animationRef.current) {
        cancelAnimationFrame(animationRef.current);
      }
      if (backendRef.current) {
        backendRef.current.destroy();
        backendRef.current = null;
      }
    };
  }, []);

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
      console.log('[ROM] Creating fresh backend...');
      backend.destroy();
      const freshBackend = createBackend(backendTypeRef.current);
      await freshBackend.init();
      backendRef.current = freshBackend;
      console.log('[ROM] Fresh backend ready, calling loadRom...');

      const result = await freshBackend.loadRom(data);
      console.log(`[ROM] loadRom returned: ${result}`);

      if (result === 0) {
        console.log("ROM loaded successfully, calling powerOn...");
        freshBackend.powerOn();

        setRomLoaded(true);
        setIsRunning(true); // Auto-start
        setError(null);
      } else {
        setError(`Failed to load ROM: error code ${result}`);
      }
    } catch (err) {
      console.error('[ROM] Error during load:', err);
      if (err instanceof Error) {
        console.error('[ROM] Stack:', err.stack);
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

    let accumulator = 0;
    let slowFrameCount = 0;
    let totalFrames = 0;

    const loop = () => {
      const backend = backendRef.current;
      if (!backend) return;

      try {
        // Run frames based on speed multiplier
        // Halve speed since requestAnimationFrame runs at ~120fps on high refresh displays
        // Use accumulator for fractional speeds
        accumulator += speedRef.current / 2;
        while (accumulator >= 1) {
          const t0 = performance.now();
          backend.runFrame();
          const elapsed = performance.now() - t0;
          totalFrames++;

          // Detect slow frames (>100ms means we're blocking the UI thread)
          if (elapsed > 100) {
            slowFrameCount++;
            const emu = (backend as any).emu;
            const status = emu?.debug_status?.() ?? 'N/A';
            console.warn(
              `[EMU] Slow frame #${slowFrameCount}: ${elapsed.toFixed(0)}ms (frame ${totalFrames}) status: ${status}`
            );
            // If too many consecutive slow frames, something is wrong
            if (slowFrameCount >= 5) {
              console.error(
                `[EMU] ${slowFrameCount} slow frames detected — possible infinite loop. Stopping.`
              );
              // Don't schedule next frame to prevent complete freeze
              return;
            }
          } else {
            slowFrameCount = 0; // Reset on good frame
          }

          accumulator -= 1;
        }

        // Render
        renderFrame();
      } catch (e) {
        // Backend was destroyed during frame - safe to ignore
        console.warn("Frame error (safe to ignore during backend switch):", e);
        return;
      }

      // Calculate FPS
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
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("keyup", handleKeyUp);

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("keyup", handleKeyUp);
    };
  }, [romLoaded, backendType]);

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
    }
  }, []);

  // Calculate container width based on fullscreen mode
  const containerWidth = fullscreen
    ? "min(420px, 95vw, calc(95vh * 0.45))"
    : "360px";

  return (
    <div
      className={className}
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        gap: fullscreen ? "0.5rem" : "1rem",
        ...(fullscreen && {
          transform: "scale(1)",
          transformOrigin: "center center",
        }),
      }}
    >
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
          {/* Calculator container with screen and keypad */}
          <div
            style={{
              background: "#1B1B1B",
              borderRadius: fullscreen ? "12px" : "16px",
              padding: fullscreen ? "12px" : "16px",
              boxShadow: "0 8px 24px rgba(0,0,0,0.4)",
              width: containerWidth,
            }}
          >
            {/* Branding */}
            <div
              style={{
                color: "#fff",
                fontSize: "1.3rem",
                fontWeight: 600,
                letterSpacing: "0.02em",
                marginBottom: "10px",
                fontFamily: "system-ui, sans-serif",
                textAlign: "center",
              }}
            >
              <span style={{ fontWeight: 700 }}>TI-84</span>{" "}
              <span style={{ fontWeight: 300 }}>Plus CE</span>
            </div>

            {/* Screen */}
            <div
              style={{
                background: "#000",
                padding: "8px",
                borderRadius: "8px",
                marginBottom: "12px",
                position: "relative",
              }}
            >
              <canvas
                ref={canvasRef}
                width={320}
                height={240}
                style={{
                  imageRendering: "pixelated",
                  width: "100%",
                  height: "auto",
                  display: "block",
                }}
              />
              {!romLoaded && useBundledRom && (
                <div
                  style={{
                    position: "absolute",
                    inset: 0,
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
            </div>

            {/* Keypad */}
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
              <button
                onClick={handleEjectRom}
                style={{ padding: "6px 16px" }}
                title="E"
              >
                Eject
              </button>
            )}
            <div
              style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}
            >
              <input
                type="range"
                min="0.25"
                max="4"
                step="0.25"
                value={speed}
                onChange={(e) => setSpeed(parseFloat(e.target.value))}
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
            <p>Shift: 2nd | Alt: Alpha | Escape: Mode | O: ON | Space: Pause</p>
          </div>
        </>
      )}
    </div>
  );
}
