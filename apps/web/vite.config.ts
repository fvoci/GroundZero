import path from 'node:path';

import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import { svelteTesting } from '@testing-library/svelte/vite';
import { defineConfig } from 'vitest/config';

const apiUrl = process.env['VITE_API_URL'] ?? 'http://127.0.0.1:8080';

export default defineConfig({
  plugins: [tailwindcss(), svelte(), svelteTesting()],
  resolve: {
    alias: { $lib: path.resolve(import.meta.dirname, 'src/lib') },
  },
  server: {
    proxy: {
      '/api': { target: apiUrl, changeOrigin: true },
      '/healthz': { target: apiUrl, changeOrigin: true },
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['tests/setup.ts'],
    include: ['tests/**/*.test.ts'],
  },
});
