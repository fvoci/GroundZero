import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { expect, test } from 'vitest';

test('the standard browser suite always registers the 200 percent font story', () => {
  const browserEnvironment: NodeJS.ProcessEnv = {
    ...process.env,
    FVOCI_BROWSER_BASE_URL: 'http://127.0.0.1:1',
    FVOCI_BROWSER_RAW: path.resolve(import.meta.dirname, '../../../target/font-registration'),
  };
  delete browserEnvironment.FVOCI_FONT_STORYBOOK_URL;
  const result = spawnSync(
    'pnpm',
    ['exec', 'playwright', 'test', 'e2e/font-assets.spec.ts', '--list', '--project', 'desktop'],
    {
      cwd: path.resolve(import.meta.dirname, '..'),
      env: browserEnvironment,
      encoding: 'utf8',
    },
  );
  expect(result.status, result.stderr).toBe(0);
  expect(result.stdout).toContain(
    '200 percent font story covers common, rare, bold and code text without overflow',
  );
});
