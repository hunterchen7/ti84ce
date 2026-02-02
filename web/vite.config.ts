import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  assetsInclude: ['**/*.rom'],
  optimizeDeps: {
    exclude: ['emu-core']
  },
  server: {
    fs: {
      // Allow serving files from the wasm package
      allow: ['..']
    }
  }
})
