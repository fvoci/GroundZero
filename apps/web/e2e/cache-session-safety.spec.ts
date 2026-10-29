import { expect, str, test, unique } from './fixtures';

// 실제 BFCache 복원은 이 실행기의 BackForwardCacheDisabledForDelegate로 재현되지 않았다.
// 코디네이터 승인: 여기서는 일반 기록 이동, persisted=true 복원은 컴포넌트 시험이 검증한다.

test('logout hides private data while pending and after going back', async ({ signIn }) => {
  const { page, api, endTrace } = await signIn('test-user');
  const secret = `로그아웃 비밀 ${unique()}`;
  const ws = await api.createWorkspace(secret);
  await page.goto(`/w/${ws}`);
  await expect(page.locator('main').last()).toContainText(secret);
  // replace가 현재 기록을 없앤 뒤에도 그 앞의 민감 화면 복원을 검사한다.
  await page.goto('/notes');
  await expect(page.getByTestId('user-menu')).toBeVisible();
  await endTrace();
  let allowLogout!: () => void;
  let sawLogout!: () => void;
  const allowed = new Promise<void>((resolve) => {
    allowLogout = resolve;
  });
  const intercepted = new Promise<void>((resolve) => {
    sawLogout = resolve;
  });
  // 실제 요청을 잠깐 보류하고 그대로 서버로 보낸다. 응답을 대신 만들지 않는다.
  await page.route('**/auth/logout', async (route) => {
    sawLogout();
    await allowed;
    await route.continue();
  });
  const response = page.waitForResponse((r) => r.url().endsWith('/auth/logout'));
  await page.getByTestId('user-menu').click();
  await page.getByRole('menuitem', { name: '로그아웃' }).click();
  await intercepted;
  try {
    await expect(page.getByTestId('user-menu')).toHaveCount(0);
    await expect(page.getByText(secret, { exact: true })).toHaveCount(0);
  } finally {
    allowLogout();
  }
  await response;
  await expect(page.getByRole('link', { name: '로그인', exact: true })).toBeVisible();
  expect(await api.status('/api/me')).toBe(401);
  await page.goBack();
  await expect(page.getByTestId('user-menu')).toHaveCount(0);
  await expect(page.getByRole('link', { name: '로그인', exact: true })).toBeVisible();
  await expect(page.getByText(secret, { exact: true })).toHaveCount(0);
});

test('a revoked public link stays cleared after back and forward navigation', async ({
  signIn,
  anonymous,
}) => {
  const owner = await signIn('test-user');
  const ws = await owner.api.createWorkspace(`공개 복원 ${unique()}`);
  const secret = `철회 비밀 ${unique()}`;
  const doc = await owner.api.createWikiDocument(ws, secret);
  const created = await owner.api.send('POST', `/api/documents/${doc}/public-links`, {});
  const publicPath = new URL(str(created, 'url')).pathname;
  const page = await anonymous();
  await page.goto(publicPath);
  await expect(page.getByRole('heading', { name: secret })).toBeVisible();
  await page.goto('/');
  // 다른 창의 소유자가 실제 HTTP로 철회한다.
  const revoked = await owner.page.request.delete(`/api/public-links/${str(created, 'id')}`, {
    headers: { origin: new URL(owner.page.url()).origin },
  });
  expect(revoked.status()).toBe(204);
  await page.goBack();
  await expect(page.getByRole('alert')).toContainText(/만료|철회/);
  await expect(page.getByRole('heading', { name: secret })).toHaveCount(0);
  await page.goForward();
  await page.goBack();
  await expect(page.getByRole('alert')).toContainText(/만료|철회/);
  await expect(page.getByRole('heading', { name: secret })).toHaveCount(0);
});
