// Opt-in browser assertions against the built, synthetic-data Storybook (D-108).
// FVOCI_MATH_STORYBOOK_URL points at a locally served target/storybook-static.
import { expect, test } from '@playwright/test';

const storybook = process.env.FVOCI_MATH_STORYBOOK_URL;
if (storybook !== undefined) {
  const story = (name: string): string =>
    `${storybook}/iframe.html?id=documents-documentscreen--${name}&viewMode=story`;
  const mathChunk = /\/katex-[^/]+\.js(?:\?|$)/;

  test('no formula: no KaTeX renderer request', async ({ page }) => {
    const requests: string[] = [];
    page.on('request', (request) => {
      requests.push(request.url());
    });
    await page.goto(story('narrow-dark-large-text'));
    await expect(page.locator('.tiptap')).toBeVisible();
    await page.waitForLoadState('networkidle');
    expect(requests.filter((url) => /\/katex[^/]*\.(js|css)(?:\?|$)/.test(url))).toEqual([]);
  });

  test('formula story: inline and block math render', async ({ page }) => {
    const renderer = page.waitForResponse((response) => mathChunk.test(response.url()));
    await page.goto(story('owner'));
    expect((await renderer).ok()).toBe(true);
    await expect(page.locator('.tiptap .katex')).toHaveCount(2);
    await expect(page.locator('[data-type="inline-math"] .katex')).toBeVisible();
    await expect(page.locator('[data-type="block-math"] .katex')).toBeVisible();
    const fonts = await page.evaluate(() =>
      performance.getEntriesByType('resource').map((e) => e.name),
    );
    expect(fonts.filter((url) => /KaTeX.*\.(woff2?|ttf|otf)/i.test(url))).toEqual([]);
  });

  test('failed renderer: source survives and retry renders without page reload', async ({
    page,
  }) => {
    let attempts = 0;
    const documentRequests: string[] = [];
    page.on('request', (request) => {
      if (request.isNavigationRequest() && request.frame() === page.mainFrame())
        documentRequests.push(request.url());
    });
    await page.route(mathChunk, async (route) => {
      attempts += 1;
      if (attempts === 1) await route.abort('failed');
      else await route.continue();
    });
    await page.goto(story('owner'));
    const timeOrigin = await page.evaluate(() => performance.timeOrigin);
    const formula = page.locator('[data-type="inline-math"]');
    await expect(formula).toContainText('E = mc^2');
    await expect(formula.getByRole('button', { name: '다시 시도' })).toBeVisible();
    expect(attempts).toBe(1);
    await formula.getByRole('button', { name: '다시 시도' }).click();
    await expect(formula.locator('.katex')).toBeVisible();
    await page
      .locator('[data-type="block-math"]')
      .getByRole('button', { name: '다시 시도' })
      .click();
    await expect(page.locator('.tiptap .katex')).toHaveCount(2);
    expect(attempts).toBe(2);
    // framenavigated also includes Storybook's same-document history updates.
    // A reload must issue another document request and replace the time origin.
    expect(documentRequests).toHaveLength(1);
    expect(await page.evaluate(() => performance.timeOrigin)).toBe(timeOrigin);
    await expect(page.getByRole('dialog')).toHaveCount(0);
  });
}
