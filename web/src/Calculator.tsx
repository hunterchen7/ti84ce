import { useEffect, useRef, useState, useCallback } from 'react';
import { createBackend, type EmulatorBackend, type BackendType } from './emulator';

// TI-84 Plus CE keypad layout
// Maps keyboard keys to [row, col] positions
const KEY_MAP: Record<string, [number, number]> = {
  // Row 0: Graph, Trace, Zoom, Window, Y=, 2nd, Mode, Del
  'F5': [0, 0], // Graph
  'F4': [0, 1], // Trace
  'F3': [0, 2], // Zoom
  'F2': [0, 3], // Window
  'F1': [0, 4], // Y=
  'Shift': [0, 5], // 2nd
  'Escape': [0, 6], // Mode
  'Backspace': [0, 7], // Del
  'Delete': [0, 7], // Del

  // Row 1: Sto, Ln, Log, x², x⁻¹, Math, Alpha, X,T,θ,n
  'Insert': [1, 0], // Sto
  'l': [1, 1], // Ln
  'L': [1, 1], // Ln
  'g': [1, 2], // Log
  'G': [1, 2], // Log
  'q': [1, 3], // x²
  'Q': [1, 3], // x²
  'r': [1, 4], // x⁻¹
  'R': [1, 4], // x⁻¹
  'm': [1, 5], // Math
  'M': [1, 5], // Math
  'Alt': [1, 6], // Alpha
  'x': [1, 7], // X,T,θ,n
  'X': [1, 7], // X,T,θ,n

  // Row 2: 0, 1, 4, 7, ,, Sin, Apps, Stat
  '0': [2, 0],
  '1': [2, 1],
  '4': [2, 2],
  '7': [2, 3],
  ',': [2, 4],
  's': [2, 5], // Sin
  'S': [2, 5], // Sin
  'Home': [2, 6], // Apps
  'End': [2, 7], // Stat

  // Row 3: ., 2, 5, 8, (, Cos, Prgm, Vars
  '.': [3, 0],
  '2': [3, 1],
  '5': [3, 2],
  '8': [3, 3],
  '(': [3, 4],
  'c': [3, 5], // Cos
  'C': [3, 5], // Cos
  'PageDown': [3, 6], // Prgm
  'PageUp': [3, 7], // Vars

  // Row 4: (-), 3, 6, 9, ), Tan, ×, ^
  '_': [4, 0], // (-)
  '3': [4, 1],
  '6': [4, 2],
  '9': [4, 3],
  ')': [4, 4],
  't': [4, 5], // Tan
  'T': [4, 5], // Tan
  '*': [4, 6], // ×
  '^': [4, 7],

  // Row 5: Enter, +, -, *, /, Clear, Down, Right
  'Enter': [5, 0],
  '+': [5, 1],
  '-': [5, 2],
  // '*' already mapped to row 4
  '/': [5, 4],
  'Clear': [5, 5],
  'ArrowDown': [5, 6],
  'ArrowRight': [5, 7],

  // Row 6: Up, Left, ?, ?, ?, ?, ?, ?
  'ArrowUp': [6, 0],
  'ArrowLeft': [6, 1],

  // ON key - special handling
  'o': [6, 5], // ON
  'O': [6, 5], // ON
};

interface CalculatorProps {
  className?: string;
  defaultBackend?: BackendType;
}

export function Calculator({ className, defaultBackend = 'rust' }: CalculatorProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const backendRef = useRef<EmulatorBackend | null>(null);
  const animationRef = useRef<number>(0);
  const [backendType, setBackendType] = useState<BackendType>(defaultBackend);
  const [isRunning, setIsRunning] = useState(false);
  const [romLoaded, setRomLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [initialized, setInitialized] = useState(false);
  const [fps, setFps] = useState(0);
  const [backendName, setBackendName] = useState('');
  const lastFrameTime = useRef(0);
  const frameCount = useRef(0);
  const romDataRef = useRef<Uint8Array | null>(null);

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
          const result = await backend.loadRom(romDataRef.current);
          if (result === 0) {
            backend.powerOn();
            setRomLoaded(true);
          } else {
            setError(`Failed to load ROM with ${backend.name}: error code ${result}`);
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
      // Don't destroy here - will be handled on next init or unmount
    };
  }, [backendType]);

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
    if (!backend) return;

    try {
      const buffer = await file.arrayBuffer();
      const data = new Uint8Array(buffer);
      romDataRef.current = data; // Store for backend switching

      const result = await backend.loadRom(data);

      if (result === 0) {
        console.log('ROM loaded successfully, calling power_on...');
        backend.powerOn();
        console.log('power_on complete');
        setRomLoaded(true);
        setError(null);
      } else {
        setError(`Failed to load ROM: error code ${result}`);
      }
    } catch (err) {
      setError(`Failed to read ROM file: ${err}`);
    }
  }, []);

  // Render frame to canvas
  const renderFrame = useCallback(() => {
    const backend = backendRef.current;
    const canvas = canvasRef.current;
    if (!backend || !canvas) return;

    try {
      const ctx = canvas.getContext('2d');
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
      console.warn('Render error (safe to ignore during backend switch):', e);
    }
  }, []);

  // Main emulation loop
  useEffect(() => {
    if (!isRunning || !romLoaded || !backendRef.current) return;

    const loop = () => {
      const backend = backendRef.current;
      if (!backend) return;

      try {
        // Run one frame
        backend.runFrame();

        // Render
        renderFrame();
      } catch (e) {
        // Backend was destroyed during frame - safe to ignore
        console.warn('Frame error (safe to ignore during backend switch):', e);
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

    const handleKeyDown = (e: KeyboardEvent) => {
      const mapping = KEY_MAP[e.key];
      if (mapping) {
        e.preventDefault();
        backend.setKey(mapping[0], mapping[1], true);
      }
    };

    const handleKeyUp = (e: KeyboardEvent) => {
      const mapping = KEY_MAP[e.key];
      if (mapping) {
        e.preventDefault();
        backend.setKey(mapping[0], mapping[1], false);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('keyup', handleKeyUp);

    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('keyup', handleKeyUp);
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

  const handleBackendChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const newBackend = e.target.value as BackendType;
    setBackendType(newBackend);
  };

  return (
    <div className={className} style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: '1rem' }}>
      <h1>TI-84 Plus CE Emulator</h1>

      {/* Backend selector */}
      <div style={{ display: 'flex', gap: '1rem', alignItems: 'center' }}>
        <label htmlFor="backend-select">Backend:</label>
        <select
          id="backend-select"
          value={backendType}
          onChange={handleBackendChange}
          disabled={isRunning}
          style={{ padding: '0.5rem' }}
        >
          <option value="rust">Rust (Custom)</option>
          <option value="cemu">CEmu (Reference)</option>
        </select>
        {backendName && (
          <span style={{ fontSize: '0.875rem', color: '#666' }}>
            Using: {backendName}
          </span>
        )}
      </div>

      {error && (
        <div style={{ color: 'red', padding: '0.5rem', background: '#fee', borderRadius: '4px' }}>
          {error}
        </div>
      )}

      {!initialized && <p>Loading {backendType === 'rust' ? 'Rust WASM' : 'CEmu WASM'} module...</p>}

      {initialized && !romLoaded && (
        <div style={{ padding: '1rem', border: '2px dashed #ccc', borderRadius: '8px' }}>
          <label htmlFor="rom-input" style={{ cursor: 'pointer' }}>
            <p>Select a TI-84 Plus CE ROM file (.rom)</p>
            <input
              id="rom-input"
              type="file"
              accept=".rom,.bin"
              onChange={handleFileChange}
              style={{ marginTop: '0.5rem' }}
            />
          </label>
        </div>
      )}

      {romLoaded && (
        <>
          <div style={{
            background: '#000',
            padding: '1rem',
            borderRadius: '8px',
            boxShadow: '0 4px 12px rgba(0,0,0,0.3)'
          }}>
            <canvas
              ref={canvasRef}
              width={320}
              height={240}
              style={{
                imageRendering: 'pixelated',
                width: '640px',
                height: '480px'
              }}
            />
          </div>

          <div style={{ display: 'flex', gap: '1rem', alignItems: 'center' }}>
            <button onClick={() => setIsRunning(!isRunning)}>
              {isRunning ? 'Pause' : 'Run'}
            </button>
            <button onClick={handleReset}>Reset</button>
            <span style={{ fontSize: '0.875rem', color: '#666' }}>
              {fps} FPS | {backendName}
            </span>
          </div>

          <div style={{ fontSize: '0.75rem', color: '#888', maxWidth: '640px', textAlign: 'center' }}>
            <p><strong>Keyboard Controls:</strong></p>
            <p>Numbers: 0-9 | Arrows: Navigate | Enter: Enter | Backspace: Del</p>
            <p>+, -, *, / : Math operations | ( ) : Parentheses | O: ON key</p>
            <p>Shift: 2nd | Alt: Alpha | Escape: Mode</p>
          </div>
        </>
      )}
    </div>
  );
}
