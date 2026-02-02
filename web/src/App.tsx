import { Calculator } from './Calculator'
import './App.css'

function App() {
  const isDemo = window.location.pathname === '/demo';

  if (isDemo) {
    return (
      <div style={{
        position: 'fixed',
        inset: 0,
        display: 'flex',
        justifyContent: 'center',
        alignItems: 'center',
        background: '#111',
        overflowY: 'auto',
      }}>
        <Calculator useBundledRom={true} defaultBackend="cemu" fullscreen />
      </div>
    );
  }

  return (
    <div style={{ minHeight: '100vh', display: 'flex', justifyContent: 'center', padding: '2rem' }}>
      <Calculator useBundledRom={false} defaultBackend="rust" />
    </div>
  );
}

export default App
