import { useCallback, useEffect, useRef, useState } from 'react';
import { useRegisterSW } from 'virtual:pwa-register/react';
import { getStateStorage } from '../storage/StateStorage';

interface RomManifest {
  version: number;
  stateVersion: number;
}

export interface SWUpdateState {
  needRefresh: boolean;
  stateWillReset: boolean;
  startMinimized: boolean;
  updateAndReload: () => void;
  dismiss: () => void;
}

const DISMISSED_KEY = 'pwa-dismissed-version';

function getDismissedVersion(): number | null {
  const v = localStorage.getItem(DISMISSED_KEY);
  return v !== null ? Number(v) : null;
}

function setDismissedVersion(version: number): void {
  localStorage.setItem(DISMISSED_KEY, String(version));
}

async function fetchManifest(): Promise<RomManifest | null> {
  try {
    const res = await fetch('/rom-manifest.json', { cache: 'no-cache' });
    if (!res.ok) return null;
    return await res.json();
  } catch {
    return null;
  }
}

export function useServiceWorker(): SWUpdateState {
  const {
    needRefresh: [needRefresh],
    updateServiceWorker,
  } = useRegisterSW({
    onRegisteredSW(_url, registration) {
      if (registration) {
        // Check for updates every 60 minutes
        setInterval(() => registration.update(), 60 * 60 * 1000);
      }
    },
  });

  const [stateWillReset, setStateWillReset] = useState(false);
  const [dismissed, setDismissed] = useState(() => getDismissedVersion() !== null);
  const manifestRef = useRef<RomManifest | null>(null);
  const loggedCurrentRef = useRef(false);

  // Log current version on startup and refine dismissed check with actual version
  useEffect(() => {
    if (loggedCurrentRef.current) return;
    loggedCurrentRef.current = true;
    fetchManifest().then((manifest) => {
      if (!manifest) return;
      manifestRef.current = manifest;
      console.log(`[PWA] Current version: ${manifest.version} (stateVersion: ${manifest.stateVersion})`);

      // Re-check: only stay dismissed if the dismissed version covers this manifest
      const dv = getDismissedVersion();
      if (dv !== null && manifest.version <= dv) {
        setDismissed(true);
      } else {
        setDismissed(false);
      }
    });
  }, []);

  // When a new SW is waiting, check if stateVersion changed
  useEffect(() => {
    if (!needRefresh) return;

    (async () => {
      const manifest = await fetchManifest();
      if (!manifest) return;
      console.log(`[PWA] Update available: version ${manifest.version} (stateVersion: ${manifest.stateVersion})`);
      manifestRef.current = manifest;

      const storage = await getStateStorage();
      const known = await storage.getKnownStateVersion();

      // If we have a known version and it differs, saves will be reset
      if (known !== null && manifest.stateVersion !== known) {
        setStateWillReset(true);
      }

      // Check if user already dismissed this version
      const dv = getDismissedVersion();
      if (dv !== null && manifest.version <= dv) {
        setDismissed(true);
      }
    })();
  }, [needRefresh]);

  const updateAndReload = useCallback(async () => {
    const storage = await getStateStorage();

    // Clear save states if stateVersion changed
    if (stateWillReset) {
      await storage.clearAllStates().catch(() => {});
    }

    // Store the new stateVersion so we can detect future changes
    if (manifestRef.current) {
      await storage.setKnownStateVersion(manifestRef.current.stateVersion).catch(() => {});
    }

    // Activate new SW and reload (fallback to hard reload if no SW)
    await updateServiceWorker(true);
    window.location.reload();
  }, [stateWillReset, updateServiceWorker]);

  const dismiss = useCallback(() => {
    if (manifestRef.current) {
      setDismissedVersion(manifestRef.current.version);
    }
    setDismissed(true);
  }, []);

  return {
    needRefresh,
    stateWillReset,
    startMinimized: dismissed,
    updateAndReload,
    dismiss,
  };
}
