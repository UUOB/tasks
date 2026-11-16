// The front end is in app/ and builds into app/dist for the Tauri shell.
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  root: 'app',
  clearScreen: false,
  server: { port: 1421, strictPort: true, fs: { allow: ['.'] } },
  build: { outDir: 'dist', emptyOutDir: true, target: 'es2022' },
});
