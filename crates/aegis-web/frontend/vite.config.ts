import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { VitePWA } from 'vite-plugin-pwa';

export default defineConfig({
  plugins: [
    react(),
    VitePWA({
      registerType: 'autoUpdate',
      includeAssets: ['favicon.ico', 'pwa-192x192.svg', 'pwa-512x512.svg'],
      manifest: {
        name: 'AegisCore',
        short_name: 'Aegis',
        description: 'Hardened Orchestration. Shielded Intelligence. Absolute Control.',
        theme_color: '#1a1a1a',
        background_color: '#1a1a1a',
        display: 'standalone',
        icons: [
          {
            src: 'pwa-192x192.svg',
            sizes: '192x192',
            type: 'image/svg+xml',
          },
          {
            src: 'pwa-512x512.svg',
            sizes: '512x512',
            type: 'image/svg+xml',
          },
          {
            src: 'pwa-512x512.svg',
            sizes: '512x512',
            type: 'image/svg+xml',
            purpose: 'any maskable',
          },
        ],
      },
    }),
  ],
  server: {
    host: '127.0.0.1',
    port: 7438,
    hmr: false,
    proxy: {
      '/projects': 'http://127.0.0.1:7437',
      '/ws': {
        target: 'ws://127.0.0.1:7437',
        ws: true,
      },
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
  },
});
