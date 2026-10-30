// #211 / #245 A: opt-in measurements against the real T6 server, without API mocks.
// Public evidence contains aggregates only; session fixtures remain in ignored target/.
// The filename matches the existing runner's perf-baseline.spec.ts filter.
import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { expect, test } from '@playwright/test';

import { list, listField, num, str } from './fixtures';

const measurementRoot = process.env.FVOCI_MEASURE_DIR;
if (measurementRoot !== undefined) {
  const fixture: unknown = JSON.parse(
    readFileSync(path.join(measurementRoot, 'browser.json'), 'utf8'),
  );
  const origin = str(fixture, 'origin');
  const cookie = {
    name: str(fixture, 'cookie_name'),
    value: str(fixture, 'cookie'),
    url: origin,
  };

  test('source-verified parallel API dependency witnesses', () => {
    // The original jq intentionally describes the old sequential source. Keep it intact.
    // This auxiliary witness includes the item -> module arm after documents move in parallel.
    const source = readFileSync(
      path.resolve(import.meta.dirname, '../src/lib/features/home/HomeDashboard.svelte'),
      'utf8',
    );
    if (!source.includes('initialActivity')) return;
    const samples: unknown = JSON.parse(
      readFileSync(path.join(measurementRoot, 'screens.json'), 'utf8'),
    );
    const results = list(samples).map((sample) => {
      const screen = str(sample, 'screen');
      const requests = listField(sample, 'requests').map((request) => ({
        path: str(request, 'path'),
        start: num(request, 'start'),
        end: num(request, 'end'),
      }));
      const pick = (pattern: RegExp, after: number, last = false) => {
        const found = requests.filter((r) => pattern.test(r.path) && r.start >= after);
        const result = last ? found.at(-1) : found[0];
        if (result === undefined) throw new Error(`missing witness ${screen}: ${String(pattern)}`);
        return result;
      };
      const root = pick(/^\/api\/me$/, 0);
      const arms = [
        pick(/^\/api\/workspaces$/, root.end),
        pick(/^\/api\/me\/organizations$/, root.end),
      ];
      const gate = arms.reduce((a, b) => (a.end > b.end ? a : b));
      const chain = [root, gate];
      if (screen === 'home' || screen === 'activity') {
        const home = pick(/^\/api\/me\/home$/, gate.end);
        const parallel = [
          pick(/^\/api\/workspaces\/[^/]+\/calendar$/, home.end),
          pick(/^\/api\/me\/notifications$/, home.end),
          pick(/^\/api\/me\/activity$/, home.end),
        ];
        const slowest = parallel.reduce((a, b) => (a.end > b.end ? a : b));
        expect(parallel.every((r) => r.start < slowest.end)).toBe(true);
        chain.push(home, slowest);
      } else if (['board', 'gantt', 'task-detail', 'document-tree'].includes(screen)) {
        const item = pick(/^\/api\/items\/[^/]+$/, gate.end);
        const documents = pick(/^\/api\/items\/[^/]+\/documents$/, gate.end);
        const module = pick(/^\/api\/modules\/[^/]+$/, item.end);
        expect(documents.start).toBeLessThan(item.end);
        chain.push(item, module);
        let after = Math.max(documents.end, module.end);
        const patterns =
          screen === 'document-tree'
            ? [/^\/api\/items\/[^/]+\/backlinks$/]
            : screen === 'task-detail'
              ? [
                  /^\/api\/tasks\/[^/]+$/,
                  /^\/api\/items\/[^/]+\/tasks$/,
                  /^\/api\/tasks\/[^/]+\/comments$/,
                ]
              : [/^\/api\/items\/[^/]+\/recurrences\/sync$/, /^\/api\/items\/[^/]+\/tasks$/];
        for (const pattern of patterns) {
          const next = pick(
            pattern,
            after,
            screen === 'task-detail' && pattern.source.endsWith('/tasks$'),
          );
          chain.push(next);
          after = next.end;
        }
      } else {
        const patterns =
          screen === 'document-open'
            ? [/^\/api\/documents\/[^/]+$/, /^\/api\/items\/[^/]+\/documents$/]
            : screen === 'calendar'
              ? [/^\/api\/workspaces\/[^/]+\/calendar$/]
              : [/^\/api\/me\/tasks$/];
        for (const pattern of patterns) chain.push(pick(pattern, chain.at(-1)?.end ?? 0));
      }
      return { screen, repeat: num(sample, 'repeat'), depth: chain.length, chain };
    });
    writeFileSync(
      path.join(measurementRoot, 'candidate-depth.json'),
      JSON.stringify(results, null, 2),
    );
  });

  test('intent reads: click, hover then click, and unused hover, three times', async ({
    browser,
  }) => {
    test.setTimeout(180_000);
    const results = [];
    const ws = str(fixture, 'workspace');
    const item = str(fixture, 'item');
    const preparation = await browser.newContext({ baseURL: origin });
    await preparation.addCookies([cookie]);
    const itemResponse = await preparation.request.get(`/api/items/${item}`);
    expect(itemResponse.ok()).toBe(true);
    const moduleId = str(await itemResponse.json(), 'module_id');
    await preparation.close();
    const flows = [
      {
        name: 'home-to-my-tasks',
        source: '/',
        link: 'main a[href="/tasks"]',
        api: '/api/me/tasks',
        target: '/tasks',
        content: 'main [data-testid="my-tasks-counts"]',
        heading: '내 업무',
      },
      {
        name: 'list-to-item',
        source: `/w/${ws}/m/${moduleId}`,
        link: `main a[href="/w/${ws}/i/${item}"]`,
        api: `/api/items/${item}`,
        target: `/w/${ws}/i/${item}`,
        content: '#item-heading',
        heading: '',
      },
    ];
    for (const flow of flows) {
      for (const action of ['click', 'hover-click', 'hover-away']) {
        for (let repeat = 1; repeat <= 3; repeat += 1) {
          const context = await browser.newContext({
            baseURL: origin,
            locale: 'ko-KR',
            viewport: { width: 1280, height: 800 },
          });
          await context.addCookies([cookie]);
          const page = await context.newPage();
          await page.goto(flow.source);
          const link = page.locator(flow.link).first();
          await expect(link).toBeVisible();
          await page.waitForLoadState('networkidle');
          const requests: { start: number; end?: number; status?: number }[] = [];
          page.on('request', (request) => {
            if (request.method() === 'GET' && new URL(request.url()).pathname === flow.api)
              requests.push({ start: request.timing().startTime });
          });
          page.on('response', (response) => {
            if (new URL(response.url()).pathname !== flow.api) return;
            const request = requests.find(
              (row) => row.start === response.request().timing().startTime,
            );
            if (request !== undefined) {
              request.end = Date.now();
              request.status = response.status();
            }
          });
          if (action !== 'click') {
            await link.hover();
            // A fixed, reported dwell interval; this is not a performance target.
            await page.waitForTimeout(200);
          }
          const beforeClick = requests.length;
          const finishedBeforeClick = requests.filter((row) => row.status === 200).length;
          let firstContentMs: number | null = null;
          if (action === 'hover-away') {
            await page.mouse.move(0, 0);
            await page.waitForLoadState('networkidle');
          } else {
            await page.evaluate(({ content, heading, target }) => {
              let started: number | undefined;
              document.addEventListener(
                'click',
                () => {
                  started = performance.now();
                },
                { capture: true, once: true },
              );
              const check = (): void => {
                if (started === undefined) return;
                const clicked = started;
                if (!window.location.pathname.startsWith(target)) return;
                if (
                  heading !== '' &&
                  document.querySelector('main h1')?.textContent.trim() !== heading
                )
                  return;
                const found = document.querySelector(content);
                if (found === null || found.getBoundingClientRect().height === 0) return;
                observer.disconnect();
                requestAnimationFrame(() => {
                  document.documentElement.dataset.intentFirst = String(
                    performance.now() - clicked,
                  );
                });
              };
              const observer = new MutationObserver(check);
              observer.observe(document, { subtree: true, childList: true, attributes: true });
            }, flow);
            await link.click();
            await expect
              .poll(() => page.evaluate(() => document.documentElement.dataset.intentFirst))
              .toBeDefined();
            firstContentMs = await page.evaluate(() =>
              Number(document.documentElement.dataset.intentFirst),
            );
            await page.waitForLoadState('networkidle');
          }
          expect(requests.every((row) => row.status === 200)).toBe(true);
          results.push({
            flow: flow.name,
            action,
            repeat,
            dwellMs: action === 'click' ? 0 : 200,
            beforeClick,
            finishedBeforeClick,
            totalReads: requests.length,
            consumed: action === 'hover-click' && beforeClick === 1 && requests.length === 1,
            firstContentMs,
          });
          writeFileSync(
            path.join(measurementRoot, 'intent-samples.json'),
            JSON.stringify(results, null, 2),
          );
          await context.close();
        }
      }
    }
  });

  test('HTML fetch preload: reuse and a session change before app execution', async ({
    browser,
  }) => {
    test.setTimeout(90_000);
    const html = readFileSync(path.resolve(import.meta.dirname, '../index.html'), 'utf8');
    if (!html.includes('as="fetch" href="/api/me"')) {
      writeFileSync(
        path.join(measurementRoot, 'preload-samples.json'),
        JSON.stringify(
          {
            result: 'NOT RUN',
            reason: 'candidate (b) link is absent',
          },
          null,
          2,
        ),
      );
      return;
    }
    const results = [];
    for (const scenario of ['signed-in', 'signed-out', 'logout-before-app', 'login-before-app']) {
      const context = await browser.newContext({ baseURL: origin, locale: 'ko-KR' });
      if (scenario === 'signed-in' || scenario === 'logout-before-app')
        await context.addCookies([cookie]);
      const page = await context.newPage();
      const responses: { status: number; id: string | null; cache: string | null }[] = [];
      const pending: Promise<void>[] = [];
      page.on('response', (response) => {
        if (new URL(response.url()).pathname !== '/api/me') return;
        pending.push(
          (async () => {
            responses.push({
              status: response.status(),
              id: await response.headerValue('x-request-id'),
              cache: await response.headerValue('cache-control'),
            });
          })(),
        );
      });
      const changing = scenario.endsWith('-before-app');
      let release: () => void = () => undefined;
      const held = new Promise<void>((resolve) => {
        release = resolve;
      });
      if (changing) {
        // Delay only the entry JS; the server still supplies every response.
        // Routing disables the HTTP cache, so these cases check identity only.
        await page.route(/\/assets\/index-[^/]+\.js$/, async (route) => {
          await held;
          await route.continue();
        });
      }
      try {
        await page.goto('/', { waitUntil: changing ? 'commit' : 'load' });
        await page.locator('script[type="module"]').waitFor({ state: 'attached' });
        const hasPreload = await page
          .locator('link[rel="preload"][as="fetch"][href="/api/me"]')
          .count();
        expect(hasPreload).toBe(1);
        if (changing) {
          await expect.poll(() => responses.length).toBe(1);
          if (scenario === 'logout-before-app') await context.clearCookies();
          else await context.addCookies([cookie]);
          release();
        }
        await page.waitForLoadState('networkidle');
        if (scenario === 'signed-out' || scenario === 'logout-before-app') {
          // T6 measurement has no OIDC provider; the anonymous screen still has this heading.
          await expect(page.locator('main h1')).toHaveText('fvoci');
          await expect(page.getByTestId('user-menu')).toHaveCount(0);
        } else await expect(page.locator('main [data-testid="my-tasks-counts"]')).toBeVisible();
        await Promise.all(pending);
        const ids = new Set(responses.map((response) => response.id));
        results.push({
          scenario,
          responses,
          distinctServerReads: ids.size,
          reused: ids.size === 1,
        });
        writeFileSync(
          path.join(measurementRoot, 'preload-samples.json'),
          JSON.stringify(results, null, 2),
        );
      } finally {
        release();
        await context.close();
      }
    }
  });
}
