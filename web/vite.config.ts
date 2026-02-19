import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { VitePWA } from 'vite-plugin-pwa'

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    react(),
    VitePWA({
      registerType: 'prompt',
      includeAssets: [
        'calculator.svg',
        'sys84.bin',
        'buttons/*.png',
      ],
      workbox: {
        globPatterns: ['**/*.{js,css,html,svg,png,wasm,bin,ico}'],
        globIgnores: ['rom-manifest.json'],
        maximumFileSizeToCacheInBytes: 5 * 1024 * 1024,
        runtimeCaching: [
          {
            urlPattern: /\/rom-manifest\.json$/,
            handler: 'NetworkFirst',
            options: {
              cacheName: 'rom-manifest',
              networkTimeoutSeconds: 3,
            },
          },
        ],
      },
      manifest: {
        name: 'TI-84 Plus CE',
        short_name: 'TI-84 CE',
        description: 'TI-84 Plus CE Calculator Emulator',
        theme_color: '#111111',
        background_color: '#111111',
        display: 'standalone',
        start_url: '/',
        icons: [
          {
            src: '/calculator.svg',
            sizes: 'any',
            type: 'image/svg+xml',
          },
        ],
      },
      devOptions: {
        enabled: false,
      },
    }),
  ],
  assetsInclude: ['**/*.rom'],
  optimizeDeps: {
    exclude: ['emu-core']
  },
  server: {
    port: 8484,
    fs: {
      // Allow serving files from the wasm package
      allow: ['..']
    }
  }
})
