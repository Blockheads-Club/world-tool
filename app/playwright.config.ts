import { defineConfig } from '@playwright/test';

// PW_PORT lets parallel checkouts (agent worktrees) run the e2e tests side by side.
const port = Number(process.env.PW_PORT ?? 4173);

export default defineConfig({
  testDir: 'e2e',
  timeout: 40_000,
  use: { baseURL: `http://localhost:${port}`, viewport: { width: 1280, height: 800 } },
  webServer: { command: `bun run build && bun run preview --port ${port} --strictPort`, url: `http://localhost:${port}`, reuseExistingServer: !process.env.CI, timeout: 120_000 },
  projects: [{ name: 'desktop' }]
});
