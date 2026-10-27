// #88·D-96: 계정 전환 뒤 다시 읽기는 고친 입력이 앞 계정의 초안으로 남았을 때만 경고 없이 진행한다. 초안을 남기지 못하면(저장소
// 사용 불가·용량 초과) 보통 떠나기처럼 beforeunload 경고를 유지해 입력을 잃지 않게 한다. 여기서는 초안 쓰기를 실제로 실패시킨
// 뒤(form-draft 키의 setItem이 QuotaExceededError), 전환 중 다시 읽기에 경고가 뜨고, 취소(머무르기)하면 안전 상태가 그대로
// 가리고 입력이 남고 쓰기 요청이 없으며, 안전 상태의 "다시 읽기"로 다시 시도해 떠나기로 답하면 새 계정의 화면이 되는지 본다.
// 취소는 Playwright의 dialog.dismiss(사람 손이 아님)다.
import { appendFileSync } from 'node:fs';

import { draftKeys, returnToWindow, switchTo } from './account-switch-helpers';
import { expect, num, str, test, unique } from './fixtures';

test('when the draft cannot be stored, the switch-time reload warns, staying keeps the safe state and the input, and an explicit retry leaves', async ({
  signIn,
}, testInfo) => {
  test.setTimeout(180_000);
  const first = await signIn('test-user');
  const { page, api } = first;
  const second = await signIn('test-user-2');
  const tag = unique();
  const ws = await api.createWorkspace(`초안 저장 실패 ${tag}`);
  await api.ensureOrganizationMember(second.api, second.user.email);
  await api.invite(ws, second.user.email, 'member');
  await second.api.acceptInvitation(ws);
  const item = await api.createProjectItem(ws, tag);
  // B도 볼 수 있게 항목을 워크스페이스 공개로(기본은 비공개).
  const itemView = await api.get(`/api/items/${item}`);
  await api.send('PUT', `/api/items/${item}`, {
    base_version: num(itemView, 'version'),
    title: str(itemView, 'title'),
    description: str(itemView, 'description'),
    visibility: 'workspace',
  });
  const title = `초안 저장 실패 업무 ${tag}`;
  const taskId = str(await api.createTask(item, title), 'id');
  const detailPath = `/w/${ws}/i/${item}/tasks/board/t/${taskId}`;
  const edit = `A가 치고 남기지 못한 설명 ${tag}`;
  const a = str(await api.get('/api/me'), 'id');
  const b = str(await second.api.get('/api/me'), 'id');
  const dialog = page.getByRole('dialog', { name: title, exact: true });
  const description = dialog.getByLabel('설명', { exact: true });
  const safe = page.getByTestId('account-safe-state');

  // 업무 폼 초안 쓰기만 실패시킨다(다른 저장은 그대로, 읽기도 그대로).
  await page.addInitScript(() => {
    const descriptor = Object.getOwnPropertyDescriptor(Storage.prototype, 'setItem');
    const original: unknown = descriptor?.value;
    if (typeof original !== 'function') throw new Error('Storage.prototype.setItem');
    Storage.prototype.setItem = function (this: Storage, key: string, value: string) {
      if (key.includes(':form-draft:'))
        throw new DOMException('quota exceeded (e2e)', 'QuotaExceededError');
      Reflect.apply(original, this, [key, value]);
    };
  });
  const prompts: string[] = [];
  page.on('dialog', (prompt) => {
    prompts.push(prompt.type());
    // 첫 beforeunload는 "머무르기"(취소), 그다음은 "떠나기".
    if (
      prompt.type() === 'beforeunload' &&
      prompts.filter((t) => t === 'beforeunload').length === 1
    )
      void prompt.dismiss();
    else void prompt.accept();
  });
  const writes: string[] = [];
  page.on('request', (request) => {
    if (request.url().includes('/api/') && request.method() !== 'GET')
      writes.push(
        `${String(Date.now() % 100000)} ${request.method()} ${new URL(request.url()).pathname}`,
      );
  });

  await test.step('A: 설명을 치지만 초안은 남지 않는다', async () => {
    await page.goto(detailPath);
    await expect(dialog).toBeVisible();
    await description.click();
    await description.fill(edit);
    await page.waitForTimeout(500);
    expect(await draftKeys(page, a)).toEqual([]);
  });

  const other = await page.context().newPage();
  await test.step('B로 바뀐 뒤 창 복귀: 다시 읽기에 경고가 뜨고, 머무르면 안전 상태·입력이 그대로이며 쓰기 요청이 없다', async () => {
    await first.endTrace();
    expect(await switchTo(other, 'test-user-2')).toBe(b);
    writes.length = 0;
    const resetAt = String(Date.now() % 100000);
    await returnToWindow(page, testInfo, (timeout) =>
      expect(safe)
        .toBeVisible({ timeout })
        .then(
          () => true,
          () => false,
        ),
    );
    const safeAt = String(Date.now() % 100000);
    await expect.poll(() => prompts.filter((t) => t === 'beforeunload').length).toBe(1);
    appendFileSync(
      testInfo.outputPath('writes.txt'),
      `reset ${resetAt} safe ${safeAt} prompt ${String(Date.now() % 100000)} writes ${writes.join(' / ') || 'none'}\n`,
    );
    // 취소된 다시 읽기: 문서는 그대로, 안전 상태가 가리고 입력은 남아 있다.
    await page.waitForTimeout(1_500);
    expect(page.url()).toContain(`/t/${taskId}`);
    await expect(safe).toBeVisible();
    // 가려진 상태에서는 역할 기반 로케이터가 요소를 찾지 못하므로 id로 본다(숨김 + 값 보존).
    const hiddenBox = page.locator('#task-description');
    await expect(hiddenBox).toBeHidden();
    await expect(hiddenBox).toHaveValue(edit);
    expect(writes, 'writes while staying in the safe state').toEqual([]);
    await page.screenshot({ path: testInfo.outputPath('1-stayed-in-safe-state.png') });
  });

  await test.step('안전 상태의 "다시 읽기"로 다시 시도해 떠나기로 답하면 새 계정의 화면이 된다', async () => {
    await safe.getByRole('button', { name: '지금 계정으로 다시 읽기' }).click();
    await expect.poll(() => prompts.filter((t) => t === 'beforeunload').length).toBe(2);
    await expect(safe).toHaveCount(0, { timeout: 20_000 });
    await expect(dialog).toBeVisible();
    expect(str(await (await page.request.get('/api/me')).json(), 'id')).toBe(b);
    await expect(description).toHaveValue('');
    await expect(page.getByTestId('task-draft-offer')).toHaveCount(0);
    await expect(page.getByText(edit)).toHaveCount(0);
    expect(await draftKeys(page, a)).toEqual([]);
    const note = `dialogs: ${prompts.join(',')}`;
    testInfo.annotations.push({ type: 'dialogs', description: note });
    appendFileSync(testInfo.outputPath('dialog-record.txt'), `${note}\n`);
    await page.screenshot({ path: testInfo.outputPath('2-left-to-b.png') });
  });
});
