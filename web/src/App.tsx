import { Calculator } from './Calculator'
import './App.css'

function App() {
  const isSandbox = window.location.pathname === '/sandbox';

  // Sandbox mode: ROM file picker for development/testing
  if (isSandbox) {
    return (
      <div style={{ minHeight: '100vh', display: 'flex', justifyContent: 'center', padding: '2rem' }}>
        <Calculator useBundledRom={false} defaultBackend="rust" fullscreen />
      </div>
    );
  }

  // Default: Demo mode with bundled ROM
  return (
    <div style={{
      position: 'fixed',
      inset: 0,
      display: 'flex',
      justifyContent: 'center',
      alignItems: 'flex-start',
      background: '#111',
      overflowY: 'auto',
    }}>
      <div style={{ marginTop: 'auto', marginBottom: 'auto', paddingTop: '1rem', paddingBottom: '1rem' }}>
        <Calculator useBundledRom={true} defaultBackend="cemu" fullscreen />
      </div>
    </div>
  );
}

export default App
