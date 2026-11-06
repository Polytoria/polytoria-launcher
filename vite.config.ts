import { defineConfig } from 'vite';

export default defineConfig({
  base: './',
  publicDir: 'static',
  clearScreen: false,
  build: { chunkSizeWarningLimit: 800 },
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] },
  },
});
