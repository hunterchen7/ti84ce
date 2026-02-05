/**
 * IndexedDB-based storage for emulator state persistence.
 * Mirrors the StateManager pattern from iOS/Android.
 */

const DB_NAME = "calc-emulator";
const DB_VERSION = 1;
const STORE_ROMS = "roms";
const STORE_STATES = "states";
const STORE_PREFS = "preferences";

/**
 * Compute SHA-256 hash of data, truncated to 16 hex characters.
 * Matches the iOS/Android StateManager hash format.
 */
async function computeRomHash(data: Uint8Array): Promise<string> {
  // Create a new ArrayBuffer copy to satisfy crypto.subtle.digest type requirements
  const buffer = new Uint8Array(data).buffer;
  const hashBuffer = await crypto.subtle.digest("SHA-256", buffer);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  const hashHex = hashArray
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
  return hashHex.substring(0, 16);
}

/**
 * Open or create the IndexedDB database.
 */
function openDatabase(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);

    request.onerror = () => reject(request.error);
    request.onsuccess = () => resolve(request.result);

    request.onupgradeneeded = (event) => {
      const db = (event.target as IDBOpenDBRequest).result;

      // Store for cached ROM copies (keyed by hash)
      if (!db.objectStoreNames.contains(STORE_ROMS)) {
        db.createObjectStore(STORE_ROMS);
      }

      // Store for save states (keyed by ROM hash)
      if (!db.objectStoreNames.contains(STORE_STATES)) {
        db.createObjectStore(STORE_STATES);
      }

      // Store for preferences
      if (!db.objectStoreNames.contains(STORE_PREFS)) {
        db.createObjectStore(STORE_PREFS);
      }
    };
  });
}

export interface EmulatorPreferences {
  lastRomHash?: string;
  lastRomName?: string;
  preferredBackend?: "rust" | "cemu";
  autoSaveEnabled?: boolean;
}

export class StateStorage {
  private db: IDBDatabase | null = null;

  /**
   * Initialize the storage system.
   */
  async init(): Promise<void> {
    this.db = await openDatabase();
  }

  /**
   * Close the database connection.
   */
  close(): void {
    if (this.db) {
      this.db.close();
      this.db = null;
    }
  }

  /**
   * Compute the hash for a ROM.
   */
  async getRomHash(romData: Uint8Array): Promise<string> {
    return computeRomHash(romData);
  }

  /**
   * Save emulator state for a given ROM.
   */
  async saveState(romHash: string, stateData: Uint8Array): Promise<void> {
    if (!this.db) throw new Error("Storage not initialized");

    return new Promise((resolve, reject) => {
      const transaction = this.db!.transaction([STORE_STATES], "readwrite");
      const store = transaction.objectStore(STORE_STATES);
      const request = store.put(stateData, romHash);

      request.onerror = () => reject(request.error);
      request.onsuccess = () => resolve();
    });
  }

  /**
   * Load emulator state for a given ROM.
   */
  async loadState(romHash: string): Promise<Uint8Array | null> {
    if (!this.db) throw new Error("Storage not initialized");

    return new Promise((resolve, reject) => {
      const transaction = this.db!.transaction([STORE_STATES], "readonly");
      const store = transaction.objectStore(STORE_STATES);
      const request = store.get(romHash);

      request.onerror = () => reject(request.error);
      request.onsuccess = () => {
        const result = request.result;
        if (result instanceof Uint8Array) {
          resolve(result);
        } else if (result instanceof ArrayBuffer) {
          resolve(new Uint8Array(result));
        } else {
          resolve(null);
        }
      };
    });
  }

  /**
   * Delete saved state for a given ROM.
   */
  async deleteState(romHash: string): Promise<void> {
    if (!this.db) throw new Error("Storage not initialized");

    return new Promise((resolve, reject) => {
      const transaction = this.db!.transaction([STORE_STATES], "readwrite");
      const store = transaction.objectStore(STORE_STATES);
      const request = store.delete(romHash);

      request.onerror = () => reject(request.error);
      request.onsuccess = () => resolve();
    });
  }

  /**
   * Check if a saved state exists for a given ROM.
   */
  async hasState(romHash: string): Promise<boolean> {
    if (!this.db) throw new Error("Storage not initialized");

    return new Promise((resolve, reject) => {
      const transaction = this.db!.transaction([STORE_STATES], "readonly");
      const store = transaction.objectStore(STORE_STATES);
      const request = store.count(romHash);

      request.onerror = () => reject(request.error);
      request.onsuccess = () => resolve(request.result > 0);
    });
  }

  /**
   * Save preferences.
   */
  async savePreferences(prefs: EmulatorPreferences): Promise<void> {
    if (!this.db) throw new Error("Storage not initialized");

    return new Promise((resolve, reject) => {
      const transaction = this.db!.transaction([STORE_PREFS], "readwrite");
      const store = transaction.objectStore(STORE_PREFS);
      const request = store.put(prefs, "emulator");

      request.onerror = () => reject(request.error);
      request.onsuccess = () => resolve();
    });
  }

  /**
   * Load preferences.
   */
  async loadPreferences(): Promise<EmulatorPreferences> {
    if (!this.db) throw new Error("Storage not initialized");

    return new Promise((resolve, reject) => {
      const transaction = this.db!.transaction([STORE_PREFS], "readonly");
      const store = transaction.objectStore(STORE_PREFS);
      const request = store.get("emulator");

      request.onerror = () => reject(request.error);
      request.onsuccess = () => {
        resolve(request.result || {});
      };
    });
  }

  /**
   * Cache a ROM by its hash.
   */
  async cacheRom(romHash: string, romData: Uint8Array): Promise<void> {
    if (!this.db) throw new Error("Storage not initialized");

    return new Promise((resolve, reject) => {
      const transaction = this.db!.transaction([STORE_ROMS], "readwrite");
      const store = transaction.objectStore(STORE_ROMS);
      const request = store.put(romData, romHash);

      request.onerror = () => reject(request.error);
      request.onsuccess = () => resolve();
    });
  }

  /**
   * Get a cached ROM by its hash.
   */
  async getCachedRom(romHash: string): Promise<Uint8Array | null> {
    if (!this.db) throw new Error("Storage not initialized");

    return new Promise((resolve, reject) => {
      const transaction = this.db!.transaction([STORE_ROMS], "readonly");
      const store = transaction.objectStore(STORE_ROMS);
      const request = store.get(romHash);

      request.onerror = () => reject(request.error);
      request.onsuccess = () => {
        const result = request.result;
        if (result instanceof Uint8Array) {
          resolve(result);
        } else if (result instanceof ArrayBuffer) {
          resolve(new Uint8Array(result));
        } else {
          resolve(null);
        }
      };
    });
  }
}

// Singleton instance
let storageInstance: StateStorage | null = null;

/**
 * Get the singleton StateStorage instance.
 * Initializes the storage on first call.
 */
export async function getStateStorage(): Promise<StateStorage> {
  if (!storageInstance) {
    storageInstance = new StateStorage();
    await storageInstance.init();
  }
  return storageInstance;
}
