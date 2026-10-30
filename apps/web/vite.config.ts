import path from 'node:path';

import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import { svelteTesting } from '@testing-library/svelte/vite';
import { defineConfig } from 'vitest/config';

const apiUrl = process.env['VITE_API_URL'] ?? 'http://127.0.0.1:8080';

export default defineConfig({
  plugins: [
    {
      name: 'fvoci-lazy-math-renderer',
      enforce: 'pre',
      resolveId(source, importer) {
        // Keep Tiptap's node definitions/commands intact; replace only its renderer binding.
        if (
          source === 'katex' &&
          importer?.replaceAll('\\', '/').includes('/@tiptap/extension-mathematics/')
        )
          return path.resolve(import.meta.dirname, 'src/lib/features/documents/katex-view.ts');
        return undefined;
      },
    },
    tailwindcss(),
    svelte(),
    svelteTesting(),
  ],
  optimizeDeps: { exclude: ['@tiptap/extension-mathematics'] },
  resolve: {
    alias: { $lib: path.resolve(import.meta.dirname, 'src/lib') },
  },
  // 개발 서버 원점은 서버의 FVOCI_PUBLIC_URL(기본 http://127.0.0.1:5173)과 같아야 한다.
  // 쿠키·OIDC 리다이렉트·Origin 검사가 이 원점 기준이다. localhost가 아니라 127.0.0.1을 쓴다.
  server: {
    host: '127.0.0.1',
    port: 5173,
    strictPort: true,
    proxy: {
      // 실시간 공동 편집 웹소켓(/api/documents/{id}/collab)도 같은 경로로 넘긴다.
      '/api': { target: apiUrl, changeOrigin: true, ws: true },
      '/auth': { target: apiUrl, changeOrigin: true },
      '/healthz': { target: apiUrl, changeOrigin: true },
    },
  },
  test: {
    server: { deps: { inline: ['@tiptap/extension-mathematics'] } },
    environment: 'jsdom',
    globals: true,
    setupFiles: ['tests/setup.ts'],
    include: ['tests/**/*.test.ts'],
  },
});
