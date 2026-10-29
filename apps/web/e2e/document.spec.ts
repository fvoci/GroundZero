// 문서 만들기·한국어 입력·저장·새로고침·다시 열기. 실제 서버에 저장된 내용을 API로도 확인한다.
// 입력은 키보드 문자열 입력이며 OS 입력기(IME) 조합 시험이 아니다.
import { expect, str, test, unique } from './fixtures';

test('keeps a rejected document setting and compares it before an explicit retry', async ({
  signIn,
}) => {
  const { page, api } = await signIn('test-user');
  const ws = await api.createWorkspace(`설정 경합 ${unique()}`);
  const created = await api.send('POST', `/api/workspaces/${ws}/wiki/documents`, {
    request_id: crypto.randomUUID(),
    title: '설정 비교',
  });
  const doc = str(created, 'id');
  await page.goto(`/w/${ws}/wiki/${doc}`);
  await expect(page.getByRole('button', { name: '공유·설정' })).toBeVisible();
  const openingRead = page.waitForResponse(
    (res) => res.url().endsWith(`/api/documents/${doc}`) && res.request().method() === 'GET',
  );
  await page.getByRole('button', { name: '공유·설정' }).click();
  await openingRead;
  const input = page.getByRole('dialog').locator('#doc-private');
  await expect(input).not.toBeChecked();
  // Another tab's request commits after this dialog has read its previous values.
  const status = await page.evaluate(async (id) => {
    const me: unknown = await fetch('/api/me').then((res) => res.json());
    if (typeof me !== 'object' || me === null || !('id' in me) || typeof me.id !== 'string')
      throw new Error('missing current user');
    return (
      await fetch(`/api/documents/${id}/settings`, {
        method: 'PATCH',
        headers: { 'content-type': 'application/json', 'x-fvoci-expected-user': me.id },
        body: JSON.stringify({ visibility: 'private', previous_visibility: 'workspace' }),
      })
    ).status;
  }, doc);
  expect(status).toBe(200);
  let hold: () => void = () => undefined;
  let release: () => void = () => undefined;
  const held = new Promise<void>((resolve) => {
    hold = resolve;
  });
  const released = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route(`**/api/documents/${doc}/settings`, async (route) => {
    // Deliver the real server reply after exercising the pending-input leave guards.
    const response = await route.fetch();
    if (response.status() === 409) {
      hold();
      await released;
    }
    await route.fulfill({ response });
  });
  const rejected = page.waitForResponse(
    (res) => res.url().endsWith(`/documents/${doc}/settings`) && res.status() === 409,
  );
  await input.click();
  await held;
  try {
    await expect(input).toBeChecked();
    await page.keyboard.press('Escape');
    await expect(page.getByRole('alertdialog')).toBeVisible();
    await page.getByRole('button', { name: '계속 편집', exact: true }).click();
    await expect(input).toBeChecked();
    const warned = await page.evaluate(() => {
      const event = new Event('beforeunload', { cancelable: true });
      window.dispatchEvent(event);
      return event.defaultPrevented;
    });
    expect(warned).toBe(true);
  } finally {
    release();
  }
  await rejected;
  const comparison = page.getByRole('region', { name: '지금과 비교' });
  await expect(comparison).toBeVisible();
  await expect(input).toBeChecked();
  await expect(comparison.getByRole('checkbox', { name: /^비공개/ })).toBeChecked();
  expect(str(await api.get(`/api/documents/${doc}`), 'visibility')).toBe('private');
  await page.getByRole('button', { name: '다시 시도', exact: true }).click();
  await expect(comparison).toBeHidden();
  await expect(input).toBeChecked();
});

test('creates a wiki document, saves Korean text and keeps it after reload and reopen', async ({
  signIn,
}) => {
  const { page, api } = await signIn('test-user');
  const tag = unique();
  const ws = await api.createWorkspace(`문서 시험 ${tag}`);
  const title = `회의록 ${tag}`;
  const renamed = `주간 회의록 — 결정 사항 ${tag}`;
  const text = `다음 주 배포 일정을 확정했습니다. 담당자는 각자 점검 목록을 정리합니다 ${tag}`;

  await page.goto(`/w/${ws}`);
  await page.getByLabel('새 문서 제목').fill(title);
  await page.getByRole('button', { name: '문서 만들기' }).click();
  await page.waitForURL(new RegExp(`/w/${ws}/wiki/[0-9a-f-]{36}$`));
  const doc = page.url().split('/').at(-1) ?? '';

  await expect(page.getByRole('status').filter({ hasText: '실시간 연결됨' })).toBeVisible();
  await page.getByRole('textbox', { name: '제목', exact: true }).fill(renamed);
  await page.getByRole('button', { name: '제목 저장', exact: true }).click();
  await expect.poll(async () => str(await api.get(`/api/documents/${doc}`), 'title')).toBe(renamed);
  await page.getByRole('textbox', { name: '본문' }).click();
  await page.keyboard.type(text);
  page.once('dialog', async (dialog) => {
    await dialog.accept();
  });
  await page.goto(`/w/${ws}`);

  // 마지막 연결을 닫으면 실시간 방이 본문을 리비전으로 남긴다.
  await expect
    .poll(async () => JSON.stringify(await api.get(`/api/documents/${doc}`)))
    .toContain(text);
  expect(str(await api.get(`/api/documents/${doc}`), 'title')).toBe(renamed);

  // 새로고침.
  await page.goto(`/w/${ws}/wiki/${doc}`);
  await page.reload();
  await expect(page.getByRole('textbox', { name: '제목', exact: true })).toHaveValue(renamed);
  await expect(page.getByRole('textbox', { name: '본문' })).toContainText(text);

  // 워크스페이스 목록에서 다시 열기.
  await page.goto(`/w/${ws}`);
  await page.getByRole('main').getByRole('link', { name: renamed }).click();
  await expect(page).toHaveURL(new RegExp(`/wiki/${doc}$`));
  await expect(page.getByRole('textbox', { name: '본문' })).toContainText(text);
  await expect(page.getByRole('status').filter({ hasText: '실시간 연결됨' })).toBeVisible();
});
