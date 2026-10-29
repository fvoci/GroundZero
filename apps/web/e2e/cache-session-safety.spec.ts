import { expect, str, test, unique } from './fixtures';

// 실제 BFCache 복원은 이 실행기의 BackForwardCacheDisabledForDelegate로 재현되지 않았다.
// 코디네이터 승인: 여기서는 일반 기록 이동, persisted=true 복원은 컴포넌트 시험이 검증한다.

test('another tab stays hidden when its old-user response precedes logout completion', async ({
  signIn,
}) => {
  const owner = await signIn('test-user');
  const observer = await owner.context.newPage();
  await observer.goto('/notes');
  const input = observer.getByLabel('새 노트 제목');
  await input.fill('로그아웃 중 보존할 입력');
  await owner.page.goto('/notes');
  await expect(owner.page.getByTestId('user-menu')).toBeVisible();
  const userId = str(await owner.api.get('/api/me'), 'id');
  await owner.endTrace();
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  let sawLogout!: () => void;
  const started = new Promise<void>((resolve) => {
    sawLogout = resolve;
  });
  await owner.page.route('**/auth/logout', async (route) => {
    sawLogout();
    await held;
    await route.continue();
  });
  const oldResponse = observer.waitForResponse(
    (response) => response.url().endsWith('/api/me') && response.status() === 200,
  );
  try {
    await owner.page.getByTestId('user-menu').click();
    await owner.page.getByRole('menuitem', { name: '로그아웃' }).click();
    await started;
    const response = await oldResponse;
    expect(str(await response.json(), 'id')).toBe(userId);
    // 실제 응답을 읽고 브라우저가 그린 뒤 검사한다. 로그아웃 요청은 계속 서버 도달 전이다.
    await observer.evaluate(
      () =>
        new Promise<void>((resolve) => {
          requestAnimationFrame(() => {
            requestAnimationFrame(() => {
              resolve();
            });
          });
        }),
    );
    await expect(observer.getByTestId('account-safe-state')).toBeVisible();
    await expect(input).toBeHidden();
    await expect(input).toHaveValue('로그아웃 중 보존할 입력');
    const marker: unknown = await observer.evaluate((): unknown =>
      JSON.parse(localStorage.getItem('fvoci:auth-session') ?? '{}'),
    );
    expect(marker).toMatchObject({
      state: 'logout-pending',
      userId,
      startedAt: expect.any(Number),
    });
  } finally {
    release();
  }
  await expect(owner.page.getByRole('link', { name: '로그인', exact: true })).toBeVisible();
  await expect(observer.getByRole('link', { name: '새 탭에서 로그인' })).toBeVisible();
  await expect(input).toBeHidden();
  await expect(input).toHaveValue('로그아웃 중 보존할 입력');
});

test('another tab hides without focus while its session verification is held', async ({
  signIn,
}) => {
  const owner = await signIn('test-user');
  const observer = await owner.context.newPage();
  await observer.goto('/notes');
  await expect(observer.getByTestId('user-menu')).toBeVisible();
  await owner.page.goto('/notes');
  await expect(owner.page.getByTestId('user-menu')).toBeVisible();
  await owner.endTrace();
  await observer.evaluate(() => {
    document.documentElement.dataset.focusCount = '0';
    window.addEventListener('focus', () => {
      const root = document.documentElement;
      root.dataset.focusCount = String(Number(root.dataset.focusCount) + 1);
    });
  });
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  let checking!: () => void;
  const checked = new Promise<void>((resolve) => {
    checking = resolve;
  });
  await observer.route('**/api/me', async (route) => {
    checking();
    await held;
    await route.continue();
  });
  await owner.page.route('**/auth/logout', async (route) => {
    await held;
    await route.continue();
  });
  try {
    await owner.page.getByTestId('user-menu').click();
    await owner.page.getByRole('menuitem', { name: '로그아웃' }).click();
    await expect(observer.getByTestId('user-menu')).toBeHidden();
    await checked;
    await expect(observer.getByTestId('account-safe-state')).toBeVisible();
    await expect(observer.locator('html')).toHaveAttribute('data-focus-count', '0');
  } finally {
    release();
  }
  await expect(owner.page.getByRole('link', { name: '로그인', exact: true })).toBeVisible();
});

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
