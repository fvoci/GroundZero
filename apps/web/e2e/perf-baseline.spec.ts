// #210: opt-in instrumentation; the runner supplies an unchanged release server and T6 data.
// Raw request IDs, cookies and URLs stay under ignored target/, never in public evidence.
import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { expect, test, type Request } from '@playwright/test';

import { str } from './fixtures';

declare global {
  interface Window {
    perfMeaningful?: number;
  }
}

interface SampleRequest {
  method: string;
  url: string;
  path: string;
  query: Record<string, string>;
  resource: string;
  start: number;
  end: number;
  protocolStart: number;
  protocolEnd: number;
  requestBytes: number;
  responseBytes: number;
  requestId: string | null;
  status: number;
}

const measurementRoot = process.env.FVOCI_MEASURE_DIR;
if (measurementRoot !== undefined) {
  test('T6 screen baseline, three fresh browser contexts per screen', async ({ browser }) => {
    test.setTimeout(900_000);
    const fixture: unknown = JSON.parse(
      readFileSync(path.join(measurementRoot, 'browser.json'), 'utf8'),
    );
    const origin = str(fixture, 'origin');
    const ws = str(fixture, 'workspace');
    const item = str(fixture, 'item');
    const prefix = `/w/${ws}/i/${item}`;
    const screens = [
      { name: 'home', route: '/', selector: '[data-testid="my-tasks-counts"]', text: '' },
      { name: 'my-tasks', route: '/tasks', selector: '[data-testid="my-tasks-counts"]', text: '' },
      {
        name: 'board',
        route: `${prefix}/tasks/board`,
        selector: '[data-task-card]',
        text: '업무 000',
      },
      {
        name: 'gantt',
        route: `${prefix}/tasks/gantt`,
        selector: '[data-testid="task-gantt"]',
        text: '업무 000',
      },
      {
        name: 'task-detail',
        route: `${prefix}/tasks/board/t/${str(fixture, 'task')}`,
        selector: '#task-title',
        text: '',
      },
      {
        name: 'document-tree',
        route: `${prefix}/docs`,
        selector: 'main [data-testid="document-tree"]',
        text: '문서 0005',
      },
      {
        name: 'document-open',
        route: `${prefix}/d/${str(fixture, 'document')}`,
        selector: '.tiptap',
        text: '연구·프로젝트 노트',
      },
      {
        name: 'calendar',
        route: `/w/${ws}/calendar`,
        selector: '[data-testid="calendar-grid"]',
        text: '',
      },
      {
        name: 'activity',
        route: '/',
        selector: 'main ul[aria-label="최근 활동"] li time',
        text: '',
      },
    ];
    const results = [];
    for (const screen of screens) {
      for (let repeat = 1; repeat <= 3; repeat += 1) {
        const context = await browser.newContext({
          baseURL: origin,
          locale: 'ko-KR',
          viewport: { width: 1280, height: 800 },
        });
        await context.addCookies([
          { name: str(fixture, 'cookie_name'), value: str(fixture, 'cookie'), url: origin },
        ]);
        const page = await context.newPage();
        await page.addInitScript(({ selector, text, name }) => {
          const check = (): void => {
            if (window.perfMeaningful !== undefined) return;
            if (
              name === 'calendar' &&
              !performance
                .getEntriesByType('resource')
                .some((entry) => entry.name.includes('/calendar?'))
            )
              return;
            const found = Array.from(document.querySelectorAll(selector)).find(
              (el) => el.getBoundingClientRect().height > 0 && el.textContent.includes(text),
            );
            if (found !== undefined)
              requestAnimationFrame(() => {
                window.perfMeaningful ??= performance.now();
                observer.disconnect();
                resources.disconnect();
              });
          };
          const observer = new MutationObserver(check);
          const resources = new PerformanceObserver(check);
          resources.observe({ type: 'resource', buffered: true });
          observer.observe(document, {
            childList: true,
            subtree: true,
            attributes: true,
            characterData: true,
          });
        }, screen);
        const requests: SampleRequest[] = [];
        const pending: Promise<void>[] = [];
        const failures: string[] = [];
        let sockets = 0;
        let socketReceivedBytes = 0;
        let socketSentBytes = 0;
        const finished = async (request: Request): Promise<void> => {
          const response = await request.response();
          if (response === null) throw new Error('finished request has no response');
          const sizes = await request.sizes();
          const timing = request.timing();
          const url = new URL(request.url());
          requests.push({
            method: request.method(),
            url: url.href,
            path: url.pathname,
            query: Object.fromEntries(url.searchParams),
            resource: request.resourceType(),
            start: timing.startTime,
            end: timing.startTime + timing.responseEnd,
            protocolStart: timing.startTime,
            protocolEnd: timing.startTime + timing.responseEnd,
            requestBytes: sizes.requestHeadersSize + sizes.requestBodySize,
            responseBytes: sizes.responseHeadersSize + sizes.responseBodySize,
            requestId: await response.headerValue('x-request-id'),
            status: response.status(),
          });
        };
        page.on('requestfinished', (request) => {
          pending.push(finished(request));
        });
        page.on('requestfailed', (request) => {
          failures.push(new URL(request.url()).pathname);
        });
        page.on('websocket', (socket) => {
          sockets += 1;
          socket.on('framereceived', ({ payload }) => {
            socketReceivedBytes += Buffer.byteLength(payload);
          });
          socket.on('framesent', ({ payload }) => {
            socketSentBytes += Buffer.byteLength(payload);
          });
        });
        await page.goto(screen.route, { waitUntil: 'load' });
        await expect
          .poll(() => page.evaluate(() => window.perfMeaningful), { timeout: 60_000 })
          .toBeDefined();
        // Include deferred initial reads, while keeping ongoing WebSockets out of network-idle accounting.
        await page.waitForLoadState('networkidle');
        await Promise.all(pending);
        expect(failures).toEqual([]);
        expect(requests.filter((request) => request.status >= 400)).toEqual([]);
        // Use one document's performance clock for API dependency boundaries.
        // Keep protocol timing separately: it can place a sequential child's start
        // slightly before its parent's reported responseEnd (observed: 0.593ms).
        const browserTimings = await page.evaluate(() =>
          performance
            .getEntriesByType('resource')
            .flatMap((entry) =>
              entry instanceof PerformanceResourceTiming
                ? [
                    {
                      url: entry.name,
                      start: performance.timeOrigin + entry.startTime,
                      end: performance.timeOrigin + entry.responseEnd,
                    },
                  ]
                : [],
            ),
        );
        const byUrl = new Map<string, { start: number; end: number }[]>();
        for (const timing of browserTimings.sort((a, b) => a.start - b.start)) {
          const entries = byUrl.get(timing.url) ?? [];
          entries.push(timing);
          byUrl.set(timing.url, entries);
        }
        for (const request of requests
          .filter((row) => row.path.startsWith('/api/'))
          .sort((a, b) => a.protocolStart - b.protocolStart)) {
          const timing = byUrl.get(request.url)?.shift();
          if (timing === undefined)
            throw new Error(`missing browser resource timing: ${request.path}`);
          request.start = timing.start;
          request.end = timing.end;
        }
        const meaningfulMs = await page.evaluate(() => window.perfMeaningful);
        const result = {
          screen: screen.name,
          repeat,
          meaningfulMs,
          sockets,
          socketReceivedBytes,
          socketSentBytes,
          failures,
          requests: requests.sort((a, b) => a.start - b.start),
        };
        results.push(result);
        writeFileSync(path.join(measurementRoot, 'screens.json'), JSON.stringify(results, null, 2));
        await context.close();
      }
    }
  });
}
