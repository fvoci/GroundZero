import { expect, list, selectOrganization, str, test, unique } from './fixtures';

// Playwright 기본 headless shell 대신 전체 Chromium으로 실제 BFCache를 검사한다.
// 이 파일에서만 BFCache 비활성화 인자를 빼며, persisted 단언은 생략하지 않는다.
test.use({
  channel: 'chromium',
  launchOptions: { ignoreDefaultArgs: ['--disable-back-forward-cache'] },
});

test('restores the same user from real BFCache without another user workspace', async ({
  signIn,
}, testInfo) => {
  const first = await signIn('test-user');
  const other = await signIn('test-user-2');
  const tag = unique();
  const ownName = `BFCache A ${tag}`;
  const foreignName = `BFCache B ${tag}`;
  await first.api.ensureOrganizationMember(other.api, other.user.email);
  await first.api.createWorkspace(ownName);
  const foreign = await other.api.createWorkspace(foreignName);
  const viewer = str(await first.api.get('/api/me'), 'id');
  expect(list(await first.api.get('/api/workspaces')).some((w) => str(w, 'id') === foreign)).toBe(
    false,
  );
  const { page } = first;
  await page.goto('/notes');
  await expect(page.getByTestId('user-menu')).toBeVisible();
  await selectOrganization(page, await first.api.initialOrganization());
  await expect(page.getByLabel('새 노트 제목')).toBeVisible();
  // trace의 CDP 수집도 끝내고, 실제 남아 있는 문서의 pageshow만 관찰한다.
  await first.endTrace();
  await other.endTrace();
  await page.evaluate((documentId) => {
    document.documentElement.dataset.bfcacheDocument = documentId;
    window.addEventListener('pageshow', (event) => {
      document.documentElement.dataset.bfcachePersisted = String(event.persisted);
    });
  }, tag);
  await page.goto('about:blank');
  await page.goBack({ waitUntil: 'commit' });
  await testInfo.attach('bfcache-navigation.json', {
    body: await page.evaluate(() => JSON.stringify(performance.getEntriesByType('navigation'))),
    contentType: 'application/json',
  });
  await expect(page.locator('html')).toHaveAttribute('data-bfcache-document', tag);
  await expect(page.locator('html')).toHaveAttribute('data-bfcache-persisted', 'true');
  await expect(page.getByLabel('새 노트 제목')).toBeVisible();
  expect(str(await first.api.get('/api/me'), 'id')).toBe(viewer);
  await page.getByTestId('workspace-switcher').click();
  await expect(page.getByRole('menuitem', { name: ownName, exact: true })).toBeVisible();
  await expect(page.getByRole('menuitem', { name: foreignName, exact: true })).toHaveCount(0);
});
