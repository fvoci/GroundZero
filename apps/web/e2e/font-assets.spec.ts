import type { Browser, Page } from '@playwright/test';

import { expect, str, test, unique, type Session } from './fixtures';
import { measureWrap, pageOverflows } from './layout';

async function coldPage(browser: Browser, session: Session): Promise<Page> {
  // Cookie inspection belongs outside the source session's public failure trace.
  await session.endTrace();
  const context = await browser.newContext();
  await context.addCookies(await session.context.cookies());
  return context.newPage();
}

function fontRequests(page: Page): string[] {
  const requests: string[] = [];
  page.on('request', (request) => {
    const name = new URL(request.url()).pathname.split('/').at(-1) ?? '';
    if (/\.(?:otf|ttf|woff2?)$/.test(name)) requests.push(name);
  });
  return requests;
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

test('cold first screen requests only one common Sans font per weight', async ({
  signIn,
  browser,
}) => {
  const session = await signIn('test-user');
  const page = await coldPage(browser, session);
  try {
    const requests = fontRequests(page);
    await page.goto(new URL('/', session.page.url()).href);
    await expect(page.getByTestId('my-tasks-counts')).toBeVisible();
    await page.waitForLoadState('networkidle');
    expectCommonOnly(requests);
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
    const requests = fontRequests(page);
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
  } finally {
    await page.context().close();
  }
});

const storybook = process.env.FVOCI_FONT_STORYBOOK_URL;
if (storybook !== undefined) {
  test('200 percent font story covers common, rare, bold and code text without overflow', async ({
    page,
  }) => {
    await page.setViewportSize({ width: 375, height: 812 });
    const requests = fontRequests(page);
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
      expect(
        requests.filter((name) => name.startsWith(`NotoSansKR-${weight}-common-`)),
      ).toHaveLength(1);
      expect(requests.filter((name) => name.startsWith(`NotoSansKR-${weight}-rest-`))).toHaveLength(
        1,
      );
    }
    expect(requests.filter((name) => name.startsWith('D2Coding-Regular-'))).toHaveLength(1);
    const screenshot = test.info().outputPath('font-200-percent.png');
    await editor.screenshot({ path: screenshot });
    await test.info().attach('font-200-percent', { path: screenshot, contentType: 'image/png' });
  });
}
