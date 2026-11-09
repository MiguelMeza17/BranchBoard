import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    host: '127.0.0.1',
    port: 4300,
    // Dentro de Tauri el puerto tiene que ser fijo (tauri.conf.json apunta a él).
    strictPort: !!process.env.TAURI_ENV_PLATFORM,
    open: false,
  },
});
