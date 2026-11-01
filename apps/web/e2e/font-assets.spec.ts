import type { Browser, Page, Request } from '@playwright/test';

import { expect, str, test, unique, type Session } from './fixtures';
import { measureWrap, pageOverflows } from './layout';

async function coldPage(browser: Browser, session: Session): Promise<Page> {
  // Cookie inspection belongs outside the source session's public failure trace.
  await session.endTrace();
  const context = await browser.newContext();
  await context.addCookies(await session.context.cookies());
  return context.newPage();
}

function fontTraffic(page: Page) {
  const names: string[] = [];
  const records = new Map<
    Request,
    { name: string; status?: number; failed?: string; finished: boolean }
  >();
  page.on('request', (request) => {
    const name = new URL(request.url()).pathname.split('/').at(-1) ?? '';
    if (/\.(?:otf|ttf|woff2?)$/.test(name)) {
      names.push(name);
      records.set(request, { name, finished: false });
    }
  });
  page.on('response', (response) => {
    const record = records.get(response.request());
    if (record) record.status = response.status();
  });
  page.on('requestfailed', (request) => {
    const record = records.get(request);
    if (record) record.failed = request.failure()?.errorText ?? 'requestfailed';
  });
  page.on('requestfinished', (request) => {
    const record = records.get(request);
    if (record) record.finished = true;
  });
  return { names, records };
}

function expectCommonOnly(requests: string[]): void {
  expect(requests).toHaveLength(2);
  for (const weight of ['Regular', 'Bold']) {
    expect(requests.filter((name) => name.startsWith(`NotoSansKR-${weight}-common-`))).toHaveLength(
      1,
    );
  }
}

async function fontsPainted(page: Page): Promise<void> {
  await page.evaluate(async () => {
    // networkidle was already reached at navigation; edits need a new layout/paint.
    await new Promise<void>((resolve) => {
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          resolve();
        });
      });
    });
    await document.fonts.ready;
  });
}

async function assertFontDelivery(
  page: Page,
  traffic: ReturnType<typeof fontTraffic>,
  samples = [{ font: '400 16px "Noto Sans KR"', text: '한글 ABC', faces: 1 }],
): Promise<void> {
  await fontsPainted(page);
  expect(traffic.records.size).toBeGreaterThan(0);
  for (const record of traffic.records.values()) {
    expect(record.failed, `${record.name}: requestfailed`).toBeUndefined();
    expect(record.status, `${record.name}: HTTP status`).toBe(200);
    expect(record.finished, `${record.name}: response completed`).toBe(true);
  }
  const before = [...traffic.names];
  for (const sample of samples) {
    const loaded = await page.evaluate(async ({ font, text }) => {
      const faces = await document.fonts.load(font, text);
      return {
        statuses: faces.map((face) => face.status),
        checked: document.fonts.check(font, text),
      };
    }, sample);
    expect(loaded.statuses).toEqual(Array<string>(sample.faces).fill('loaded'));
    expect(loaded.checked).toBe(true);
  }
  // Loading explicitly must not conceal a missing native layout request.
  expect(traffic.names).toEqual(before);
}

for (const failure of ['404', '500', 'requestfailed']) {
  test(`rejects font delivery ${failure}`, async ({ signIn, browser }) => {
    const session = await signIn('test-user');
    const page = await coldPage(browser, session);
    try {
      await page.route('**/NotoSansKR-Regular-common-*.woff2', async (route) => {
        if (failure === 'requestfailed') await route.abort('failed');
        else await route.fulfill({ status: Number(failure), body: '' });
      });
      const traffic = fontTraffic(page);
      await page.goto(new URL('/', session.page.url()).href);
      await expect(page.getByTestId('my-tasks-counts')).toBeVisible();
      await page.waitForLoadState('networkidle');
      await expect(assertFontDelivery(page, traffic)).rejects.toThrow(/HTTP status|requestfailed/);
    } finally {
      await page.context().close();
    }
  });
}

test('cold first screen requests only one common Sans font per weight', async ({
  signIn,
  browser,
}) => {
  const session = await signIn('test-user');
  const page = await coldPage(browser, session);
  try {
    const traffic = fontTraffic(page);
    const requests = traffic.names;
    await page.goto(new URL('/', session.page.url()).href);
    await expect(page.getByTestId('my-tasks-counts')).toBeVisible();
    await page.waitForLoadState('networkidle');
    expectCommonOnly(requests);
    await assertFontDelivery(page, traffic, [
      { font: '400 16px "Noto Sans KR"', text: '한글 ABC', faces: 1 },
      { font: '700 16px "Noto Sans KR"', text: '한글 ABC', faces: 1 },
    ]);
    expect(await page.locator('link[rel="preload"][as="font"]').count()).toBe(2);
  } finally {
    await page.context().close();
  }
});

test('rare Hangul and Hanja add one request per used weight and reuse it', async ({
  signIn,
  browser,
}) => {
  const session = await signIn('test-user');
  const workspace = await session.api.createWorkspace(`글꼴 시험 ${unique()}`);
  const created = await session.api.send('POST', `/api/workspaces/${workspace}/wiki/documents`, {
    request_id: crypto.randomUUID(),
    title: '글꼴 범위 시험',
  });
  const id = str(created, 'id');
  const page = await coldPage(browser, session);
  try {
    const traffic = fontTraffic(page);
    const requests = traffic.names;
    await page.goto(new URL(`/w/${workspace}/wiki/${id}`, session.page.url()).href);
    const editor = page.getByRole('textbox', { name: '본문', exact: true });
    await expect(editor).toBeVisible();
    await page.waitForLoadState('networkidle');
    expect(requests).toHaveLength(2);
    await editor.fill('옛한글 ᄀᆞ 龍 漢字');
    await expect(editor).toContainText('龍');
    await fontsPainted(page);
    expect(requests).toHaveLength(3);
    expect(requests.filter((name) => name.startsWith('NotoSansKR-Regular-rest-'))).toHaveLength(1);
    await editor.press('End');
    await page.keyboard.insertText(' 更多 ᄒᆞᆫ');
    await fontsPainted(page);
    expect(requests).toHaveLength(3);
    await page.keyboard.press('ControlOrMeta+b');
    await page.keyboard.insertText(' 龍 ᄀᆞ');
    await expect(editor.locator('strong')).toContainText('龍');
    await fontsPainted(page);
    expect(requests).toHaveLength(4);
    expect(requests.filter((name) => name.startsWith('NotoSansKR-Bold-rest-'))).toHaveLength(1);
    await page.keyboard.insertText(' 更多');
    await fontsPainted(page);
    expect(requests).toHaveLength(4);
    await expect(editor).toContainText('龍');
    await assertFontDelivery(page, traffic, [
      { font: '400 16px "Noto Sans KR"', text: '한글 龍 ᄀᆞ', faces: 2 },
      { font: '700 16px "Noto Sans KR"', text: '한글 龍 ᄀᆞ', faces: 2 },
    ]);
  } finally {
    await page.context().close();
  }
});

test('200 percent font story covers common, rare, bold and code text without overflow', async ({
  page,
}) => {
  const storybook = process.env.FVOCI_FONT_STORYBOOK_URL;
  if (!storybook) throw new Error('Run cargo xtask browser to build and serve Storybook');
  await page.setViewportSize({ width: 375, height: 812 });
  const traffic = fontTraffic(page);
  const requests = traffic.names;
  await page.goto(
    `${storybook}/iframe.html?id=documents-documentscreen--narrow-dark-large-text&viewMode=story&globals=fontScale:200;theme:dark`,
  );
  const editor = page.locator('.tiptap');
  await expect(editor).toContainText('옛한글');
  await expect(editor.locator('strong')).toContainText('漢字 龍');
  await expect(editor.locator('pre')).toContainText('KS X 1001');
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
  await page.waitForLoadState('networkidle');
  expect(await page.evaluate(() => getComputedStyle(document.documentElement).fontSize)).toBe(
    '32px',
  );
  const wrap = await measureWrap(editor.locator('p').first());
  expect(wrap.words).toBeGreaterThan(30);
  expect(wrap.broken).toEqual([]);
  expect(wrap.overflow).toBe(false);
  expect(await pageOverflows(page)).toBe(false);
  for (const weight of ['Regular', 'Bold']) {
    expect(requests.filter((name) => name.startsWith(`NotoSansKR-${weight}-common-`))).toHaveLength(
      1,
    );
    expect(requests.filter((name) => name.startsWith(`NotoSansKR-${weight}-rest-`))).toHaveLength(
      1,
    );
  }
  expect(requests.filter((name) => name.startsWith('D2Coding-Regular-'))).toHaveLength(1);
  await assertFontDelivery(page, traffic, [
    { font: '400 16px "Noto Sans KR"', text: '한글 龍 ᄀᆞ', faces: 2 },
    { font: '700 16px "Noto Sans KR"', text: '한글 龍 ᄀᆞ', faces: 2 },
    { font: '400 16px D2Coding', text: 'KS X 1001', faces: 1 },
  ]);
  const screenshot = test.info().outputPath('font-200-percent.png');
  await editor.screenshot({ path: screenshot });
  await test.info().attach('font-200-percent', { path: screenshot, contentType: 'image/png' });
});
