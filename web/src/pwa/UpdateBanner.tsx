import { useState } from 'react';
import { useServiceWorker } from './useServiceWorker';

export function UpdateBanner() {
  const { needRefresh, stateWillReset, startMinimized, updateAndReload, dismiss } = useServiceWorker();
  const [confirming, setConfirming] = useState(false);
  const [minimized, setMinimized] = useState(startMinimized);
  const [dontShowAgain, setDontShowAgain] = useState(false);

  if (!needRefresh) return null;

  const handleUpdate = () => {
    if (stateWillReset && !confirming) {
      setConfirming(true);
      return;
    }
    updateAndReload();
  };

  const handleHide = () => {
    if (dontShowAgain) {
      dismiss();
    }
    setMinimized(true);
    setConfirming(false);
  };

  // Minimized pill in bottom-right corner
  if (minimized) {
    return (
      <button
        onClick={() => setMinimized(false)}
        title="Update available"
        style={{
          position: 'fixed',
          bottom: 12,
          right: 12,
          zIndex: 10000,
          background: stateWillReset ? '#b63' : '#4a9eff',
          color: '#fff',
          border: 'none',
          borderRadius: '50%',
          width: 36,
          height: 36,
          fontSize: '18px',
          lineHeight: 1,
          cursor: 'pointer',
          boxShadow: '0 2px 8px rgba(0,0,0,0.4)',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
        }}
      >
        &#x21bb;
      </button>
    );
  }

  // Full banner
  return (
    <div style={{
      position: 'fixed',
      bottom: 0,
      left: 0,
      right: 0,
      zIndex: 10000,
      background: '#1a1a1a',
      borderTop: stateWillReset ? '1px solid #b33' : '1px solid #333',
      padding: '10px 16px',
      display: 'flex',
      flexDirection: 'column',
      alignItems: 'center',
      gap: '8px',
      fontFamily: 'system-ui, sans-serif',
      fontSize: '14px',
      color: '#e0e0e0',
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
        <span>
          {confirming
            ? 'Are you sure? Your save state will be permanently deleted.'
            : stateWillReset
              ? 'New version available. Updating will erase your save data.'
              : 'A new version is available.'}
        </span>
        <button onClick={handleUpdate} style={{
          padding: '6px 16px',
          background: confirming ? '#c33' : stateWillReset ? '#b63' : '#4a9eff',
          color: '#fff',
          border: 'none',
          borderRadius: '4px',
          cursor: 'pointer',
          fontSize: '14px',
        }}>
          {confirming ? 'Yes, update' : 'Update'}
        </button>
        <button onClick={confirming ? () => setConfirming(false) : handleHide} style={{
          padding: '6px 16px',
          background: 'transparent',
          color: '#888',
          border: '1px solid #555',
          borderRadius: '4px',
          cursor: 'pointer',
          fontSize: '14px',
        }}>
          {confirming ? 'Cancel' : 'Hide'}
        </button>
      </div>
      {!confirming && (
        <label style={{
          display: 'flex',
          alignItems: 'center',
          gap: '6px',
          fontSize: '12px',
          color: '#888',
          cursor: 'pointer',
        }}>
          <input
            type="checkbox"
            checked={dontShowAgain}
            onChange={(e) => setDontShowAgain(e.target.checked)}
            style={{ accentColor: '#4a9eff' }}
          />
          Don't show again for this version
        </label>
      )}
    </div>
  );
}
