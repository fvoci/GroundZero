import { expect, str, test, unique } from './fixtures';

// 실제 BFCache 복원은 이 실행기의 BackForwardCacheDisabledForDelegate로 재현되지 않았다.
// 코디네이터 승인: 여기서는 일반 기록 이동, persisted=true 복원은 컴포넌트 시험이 검증한다.

for (const withoutLocks of [false, true]) {
  test(`SR-242-1 retries get a new deadline after failed logout (Web Locks ${withoutLocks ? 'missing' : 'available'})`, async ({
    signIn,
  }) => {
    const owner = await signIn('test-user');
    if (withoutLocks)
      await owner.context.addInitScript(() => {
        Object.defineProperty(navigator, 'locks', { configurable: true, value: undefined });
      });
    const observer = await owner.context.newPage();
    await observer.goto('/notes');
    const input = observer.getByLabel('새 노트 제목');
    await input.fill('두 번째 시도 뒤 보존할 입력');
    await owner.page.goto('/notes');
    await expect(owner.page.getByTestId('user-menu')).toBeVisible();
    await owner.endTrace();
    const now = new Date();
    await owner.page.clock.install({ time: now });
    await observer.clock.install({ time: now });
    let failFirst!: () => void;
    const firstFailure = new Promise<void>((resolve) => {
      failFirst = resolve;
    });
    let allowCommit!: () => void;
    const allowed = new Promise<void>((resolve) => {
      allowCommit = resolve;
    });
    let committed!: () => void;
    const commit = new Promise<void>((resolve) => {
      committed = resolve;
    });
    let attempts = 0;
    await owner.page.route('**/auth/logout', async (route) => {
      attempts += 1;
      if (attempts === 1) {
        await firstFailure;
        await route.abort('failed');
        return;
      }
      await allowed;
      const response = await route.fetch();
      expect(response.status()).toBe(200);
      committed();
    });
    const first = observer.waitForResponse(
      (r) => r.url().endsWith('/api/me') && r.status() === 200,
    );
    await owner.page.getByTestId('user-menu').click();
    await owner.page.getByRole('menuitem', { name: '로그아웃' }).click();
    await first;
    failFirst();
    const retry = owner.page.getByRole('button', { name: '로그아웃 다시 하기' });
    await expect(retry).toBeVisible();
    await owner.page.clock.runFor(10_000);
    await observer.clock.runFor(10_000);
    const second = observer.waitForResponse(
      (r) => r.url().endsWith('/api/me') && r.status() === 200,
    );
    await retry.click();
    await second;
    await observer.clock.runFor(20_000); // 첫 시도의 기한, 두 번째 요청은 아직 커밋 전.
    await expect(input).toBeVisible();
    allowCommit();
    await commit;
    expect(await owner.api.status('/api/me')).toBe(401);
    await owner.page.close();
    const anonymous = observer.waitForResponse(
      (r) => r.url().endsWith('/api/me') && r.status() === 401,
    );
    await observer.clock.runFor(10_000);
    await anonymous;
    await expect(observer.getByRole('link', { name: '새 탭에서 로그인' })).toBeVisible();
    await expect(input).toBeHidden();
    await expect(input).toHaveValue('두 번째 시도 뒤 보존할 입력');
  });

  test(`SR-242-1 mounts after pending and verifies without a final signal (Web Locks ${withoutLocks ? 'missing' : 'available'})`, async ({
    signIn,
  }) => {
    const owner = await signIn('test-user');
    if (withoutLocks)
      await owner.context.addInitScript(() => {
        Object.defineProperty(navigator, 'locks', { configurable: true, value: undefined });
      });
    await owner.page.goto('/notes');
    await expect(owner.page.getByTestId('user-menu')).toBeVisible();
    await owner.endTrace();
    let started!: () => void;
    const pending = new Promise<void>((resolve) => {
      started = resolve;
    });
    let allowCommit!: () => void;
    const allowed = new Promise<void>((resolve) => {
      allowCommit = resolve;
    });
    let committed!: () => void;
    const commit = new Promise<void>((resolve) => {
      committed = resolve;
    });
    await owner.page.route('**/auth/logout', async (route) => {
      started();
      await allowed;
      const response = await route.fetch();
      expect(response.status()).toBe(200);
      committed();
    });
    await owner.page.getByTestId('user-menu').click();
    await owner.page.getByRole('menuitem', { name: '로그아웃' }).click();
    await pending;
    const observer = await owner.context.newPage();
    await observer.clock.install();
    await observer.goto('/notes');
    const input = observer.getByLabel('새 노트 제목');
    await input.fill('늦게 열린 탭의 입력');
    allowCommit();
    await commit;
    expect(await owner.api.status('/api/me')).toBe(401);
    await owner.page.close();
    const anonymous = observer.waitForResponse(
      (r) => r.url().endsWith('/api/me') && r.status() === 401,
    );
    await observer.clock.runFor(30_000);
    await anonymous;
    await expect(observer.getByRole('link', { name: '새 탭에서 로그인' })).toBeVisible();
    await expect(input).toBeHidden();
    await expect(input).toHaveValue('늦게 열린 탭의 입력');
  });

  test(`SR-221-12 verifies after the logout tab closes after server commit (Web Locks ${withoutLocks ? 'missing' : 'available'})`, async ({
    signIn,
  }) => {
    const owner = await signIn('test-user');
    if (withoutLocks)
      await owner.context.addInitScript(() => {
        Object.defineProperty(navigator, 'locks', { configurable: true, value: undefined });
      });
    const observer = await owner.context.newPage();
    await observer.goto('/notes');
    const input = observer.getByLabel('새 노트 제목');
    await input.fill('닫힌 탭 뒤에도 보존할 입력');
    await owner.page.goto('/notes');
    await expect(owner.page.getByTestId('user-menu')).toBeVisible();
    await owner.endTrace();
    await observer.clock.install();
    let allowCommit!: () => void;
    const allowed = new Promise<void>((resolve) => {
      allowCommit = resolve;
    });
    let committed!: () => void;
    const commit = new Promise<void>((resolve) => {
      committed = resolve;
    });
    await owner.page.route('**/auth/logout', async (route) => {
      await allowed;
      const response = await route.fetch();
      expect(response.status()).toBe(200);
      committed();
      // 서버 커밋 뒤 응답 전달 전에 탭을 닫아 최종 게시를 없앤다.
    });
    const first = observer.waitForResponse(
      (r) => r.url().endsWith('/api/me') && r.status() === 200,
    );
    await owner.page.getByTestId('user-menu').click();
    await owner.page.getByRole('menuitem', { name: '로그아웃' }).click();
    await first;
    await expect(input).toBeVisible();
    allowCommit();
    await commit;
    expect(await owner.api.status('/api/me')).toBe(401);
    await owner.page.close();
    const anonymous = observer.waitForResponse(
      (r) => r.url().endsWith('/api/me') && r.status() === 401,
    );
    await observer.clock.runFor(30_000);
    await anonymous;
    await expect(observer.getByRole('link', { name: '새 탭에서 로그인' })).toBeVisible();
    await expect(input).toBeHidden();
    await expect(input).toHaveValue('닫힌 탭 뒤에도 보존할 입력');
  });

  test(`SR-221-11 notifies after failed logout (Web Locks ${withoutLocks ? 'missing' : 'available'})`, async ({
    signIn,
  }) => {
    const owner = await signIn('test-user');
    if (withoutLocks)
      await owner.context.addInitScript(() => {
        Object.defineProperty(navigator, 'locks', { configurable: true, value: undefined });
      });
    const observer = await owner.context.newPage();
    await observer.goto('/notes');
    const input = observer.getByLabel('새 노트 제목');
    await input.fill('실패 뒤 보존할 입력');
    await owner.page.goto('/notes');
    await expect(owner.page.getByTestId('user-menu')).toBeVisible();
    await owner.endTrace();
    let fail!: () => void;
    const failure = new Promise<void>((resolve) => {
      fail = resolve;
    });
    await owner.page.route('**/auth/logout', async (route) => {
      await failure;
      await route.abort('failed');
    });
    const first = observer.waitForResponse(
      (r) => r.url().endsWith('/api/me') && r.status() === 200,
    );
    await owner.page.getByTestId('user-menu').click();
    await owner.page.getByRole('menuitem', { name: '로그아웃' }).click();
    await first;
    await expect(input).toBeVisible();
    const followUp = observer.waitForResponse(
      (r) => r.url().endsWith('/api/me') && r.status() === 200,
    );
    fail();
    await expect(owner.page.getByRole('button', { name: '로그아웃 다시 하기' })).toBeVisible();
    await followUp;
    expect(await owner.api.status('/api/me')).toBe(200);
    await expect(input).toBeVisible();
    await expect(input).toHaveValue('실패 뒤 보존할 입력');
    await expect(observer.getByTestId('account-safe-state')).toBeHidden();
  });
}

test('a late initial old-user response cannot overwrite the logout completion marker', async ({
  signIn,
}) => {
  const owner = await signIn('test-user');
  await owner.page.goto('/notes');
  await expect(owner.page.getByTestId('user-menu')).toBeVisible();
  const userId = str(await owner.api.get('/api/me'), 'id');
  await owner.endTrace();
  const late = await owner.context.newPage();
  // 이 새 문서의 표식 게시를 관찰한다. 세대가 지난 응답은 신원 없이도 게시하지 않는다.
  await late.addInitScript(() => {
    const original: unknown = Object.getOwnPropertyDescriptor(Storage.prototype, 'setItem')?.value;
    if (typeof original !== 'function') throw new Error('Storage.prototype.setItem');
    Storage.prototype.setItem = function (this: Storage, key: string, value: string) {
      const result: unknown = Reflect.apply(original, this, [key, value]);
      try {
        const marker: unknown = JSON.parse(value);
        if (key === 'fvoci:auth-session' && typeof marker === 'object' && marker !== null) {
          document.documentElement.dataset.sessionSignalPublished = 'yes';
        }
      } catch {
        /* 관찰 오류는 원래 쓰기의 결과를 바꾸지 않는다. */
      }
      return result;
    };
  });
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  let captured!: () => void;
  const capturedOldUser = new Promise<void>((resolve) => {
    captured = resolve;
  });
  await late.route('**/api/me', async (route) => {
    const response = await route.fetch();
    if (response.status() === 200) {
      expect(str(await response.json(), 'id')).toBe(userId);
      captured();
      await held;
    }
    // 실제 서버 응답의 상태·머리글·본문을 그대로, 전달 시점만 늦춘다.
    await route.fulfill({ response });
  });
  const anonymousResponse = late.waitForResponse(
    (response) => response.url().endsWith('/api/me') && response.status() === 401,
  );
  const navigation = late.goto('/notes');
  try {
    await capturedOldUser;
    await owner.page.getByTestId('user-menu').click();
    await owner.page.getByRole('menuitem', { name: '로그아웃' }).click();
    await expect(owner.page.getByRole('link', { name: '로그인', exact: true })).toBeVisible();
  } finally {
    release();
  }
  await navigation;
  // 첫 A 응답을 수락한 뒤, 대기 알림의 후속 익명 응답으로 재인증 상태가 된다.
  await anonymousResponse;
  await expect(late.getByRole('link', { name: '새 탭에서 로그인' })).toBeVisible();
  await expect(late.getByTestId('user-menu')).toBeHidden();
  await expect(late.locator('html')).not.toHaveAttribute('data-session-signal-published', 'yes');
  const final: unknown = await late.evaluate((): unknown =>
    JSON.parse(localStorage.getItem('fvoci:auth-session') ?? '{}'),
  );
  expect(final).toMatchObject({ state: 'settled' });
  expect(final).not.toHaveProperty('userId');
});

test('another tab accepts its own old-user response then verifies logout completion', async ({
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
    await expect(observer.getByTestId('account-safe-state')).toBeHidden();
    await expect(input).toBeVisible();
    await expect(input).toHaveValue('로그아웃 중 보존할 입력');
    const marker: unknown = await observer.evaluate((): unknown =>
      JSON.parse(localStorage.getItem('fvoci:auth-session') ?? '{}'),
    );
    expect(marker).toMatchObject({
      state: 'logout-pending',
      startedAt: expect.any(Number),
    });
    expect(marker).not.toHaveProperty('userId');
  } finally {
    release();
  }
  await expect(owner.page.getByRole('link', { name: '로그인', exact: true })).toBeVisible();
  await expect(observer.getByRole('link', { name: '새 탭에서 로그인' })).toBeVisible();
  await expect(input).toBeHidden();
  await expect(input).toHaveValue('로그아웃 중 보존할 입력');
});

test('another tab verifies without focus and masks only after its own anonymous response', async ({
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
    await checked;
    await expect(observer.getByTestId('user-menu')).toBeVisible();
    await expect(observer.getByTestId('account-safe-state')).toBeHidden();
    await expect(observer.locator('html')).toHaveAttribute('data-focus-count', '0');
  } finally {
    release();
  }
  await expect(owner.page.getByRole('link', { name: '로그인', exact: true })).toBeVisible();
  await expect(observer.getByRole('link', { name: '새 탭에서 로그인' })).toBeVisible();
  await expect(observer.getByTestId('user-menu')).toBeHidden();
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
