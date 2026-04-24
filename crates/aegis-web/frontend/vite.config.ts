import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    host: '127.0.0.1',
    port: 7438,
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
