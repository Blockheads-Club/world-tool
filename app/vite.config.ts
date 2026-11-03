import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';

const host = process.env.TAURI_DEV_HOST;
// Tests format dates in the local zone; pin it so they pass on runners outside UTC.
if (process.env.VITEST) process.env.TZ = 'UTC';

export default defineConfig({
  plugins: [sveltekit()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: { ignored: ['**/src-tauri/**'] }
  },
  resolve: process.env.VITEST ? { conditions: ['browser'] } : undefined,
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts'],
    setupFiles: ['src/test-setup.ts'],
    testTimeout: 20_000,
    coverage: {
      provider: 'v8',
      include: ['src/lib/**/*.{ts,svelte}', 'src/routes/**/*.{ts,svelte}'],
      exclude: ['src/**/*.test.ts', 'src/lib/test-utils.ts', 'src/lib/types.ts'],
      reporter: ['text', 'text-summary'],
      thresholds: { lines: 100, branches: 100, functions: 100, statements: 100 }
    }
  }
});
