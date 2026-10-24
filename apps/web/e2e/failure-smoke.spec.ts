// 실패 증거 수집과 공개 검사의 확인(의도적으로 실패한다). FVOCI_BROWSER_FAILURE_SMOKE가 있을 때만 돈다(playwright.config.ts).
// - clean: 합성 민감 값을 쿠키로만 실어 요청한 뒤 실패한다. 공개된 trace의 네트워크 기록이 빠지고 값이 없어야 한다.
// - leak: 같은 값을 화면(DOM)에도 넣고 실패한다. trace 스냅숏에 남으므로 공개를 거절하고 SAFE_TO_UPLOAD가 없어야 한다.
import { expect, recordSensitive, test, unique } from './fixtures';

const mode = process.env.FVOCI_BROWSER_FAILURE_SMOKE;

test('failure evidence smoke (intentional failure)', async ({ signIn, baseURL }) => {
  const sentinel = `fvoci-sentinel-${unique()}${unique()}`;
  recordSensitive([sentinel]);
  const { page } = await signIn('test-user', {
    beforeTrace: async (context) => {
      await context.addCookies([{ name: 'fvoci_smoke', value: sentinel, url: baseURL ?? '' }]);
    },
  });
  await page.goto('/notes');
  // 합성 쿠키가 실린 요청(네트워크 기록에만 남는다).
  expect(await page.evaluate(async () => (await fetch('/api/me')).status)).toBe(200);
  if (mode === 'leak') {
    await page.evaluate((s) => {
      document.body.dataset.smoke = s;
    }, sentinel);
  }
  await expect(page.getByText('의도적인 실패: 이 문구는 화면에 없다')).toBeVisible({
    timeout: 1000,
  });
});
