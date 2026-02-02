// Emulator backend exports

export type { EmulatorBackend, BackendType } from './types';
export { RustBackend } from './RustBackend';
export { CEmuBackend } from './CEmuBackend';

import type { EmulatorBackend, BackendType } from './types';
import { RustBackend } from './RustBackend';
import { CEmuBackend } from './CEmuBackend';

export function createBackend(type: BackendType): EmulatorBackend {
  switch (type) {
    case 'rust':
      return new RustBackend();
    case 'cemu':
      return new CEmuBackend();
    default:
      throw new Error(`Unknown backend type: ${type}`);
  }
}
