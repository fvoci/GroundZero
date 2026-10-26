// D-96·D-128(PR #141 r4372766851): 업무 폼의 저장하지 않은 입력은 같은 브라우저에서 다른 계정으로 바뀌어 다시 읽힌 뒤에도
// 원래 계정에게만 되살린다. 실제 Keycloak 로그아웃·로그인, 실제 창 복귀 확인(FeedbackHost의 /api/me 비교), 실제 다시
// 읽기(window.location.reload)를 거친다. 다른 계정은 입력을 보지 못하고, 되살린 입력은 사용자가 직접 저장·만들기를 눌러야
// 서버로 간다(중복 저장·생성 없음). HTTP mock·저장소 주입은 없다.
import { appendFileSync } from 'node:fs';

import type { Page, TestInfo } from '@playwright/test';

import { expect, list, logIn, num, recordSensitive, str, test, testUser, unique } from './fixtures';

/** 이 브라우저에 남은 이 사용자의 폼 입력 키. */
async function draftKeys(page: Page, userId: string): Promise<string[]> {
  return page.evaluate(
    (prefix) => Object.keys(window.localStorage).filter((k) => k.startsWith(prefix)),
    `fvoci:${userId}:-:form-draft:`,
  );
}

/** 사용자 메뉴로 로그아웃하고(제공자 세션도 끝난다) 다른 계정으로 로그인한다. 인증 화면은 trace 밖이다. */
async function switchTo(page: Page, username: 'test-user' | 'test-user-2'): Promise<string> {
  await page.goto('/');
  await page.getByTestId('user-menu').click();
  await page.getByRole('menuitem', { name: '로그아웃' }).click();
  await expect(page.getByRole('link', { name: '로그인' })).toBeVisible();
  await logIn(page, testUser(username));
  recordSensitive((await page.context().cookies()).map((c) => c.value));
  return str(await (await page.request.get('/api/me')).json(), 'id');
}

/**
 * 다른 탭에서 계정을 바꾼 뒤 이 창으로 돌아온다. 탭 전환(bringToFront)이 창 focus를 내지 않는 헤드리스 환경이면 focus
 * 이벤트를 직접 낸다. 그 뒤의 확인 요청(/api/me)·계정 비교·안전 상태·다시 읽기는 모두 실제 코드다. 어느 쪽이었는지 기록한다.
 */
async function returnToWindow(page: Page, testInfo: TestInfo): Promise<void> {
  const reloaded = page.waitForEvent('load', { timeout: 5_000 }).then(
    () => true,
    () => false,
  );
  await page.bringToFront();
  const note = (text: string): void => {
    testInfo.annotations.push({ type: 'focus', description: text });
    appendFileSync(testInfo.outputPath('focus-path.txt'), `${text}\n`);
  };
  if (await reloaded) {
    note('bringToFront fired window focus');
    return;
  }
  note('headless: window focus dispatched programmatically (not a real tab switch)');
  const next = page.waitForEvent('load', { timeout: 15_000 });
  await page.evaluate(() => {
    window.dispatchEvent(new Event('focus'));
  });
  await next;
}

test('unsaved task input survives A→B→A account switches and reloads, for the first account only', async ({
  signIn,
}, testInfo) => {
  test.setTimeout(240_000);
  const first = await signIn('test-user');
  const { page, api } = first;
  // 두 번째 계정이 같은 업무를 열 수 있어야 "제안 없음"이 의미 있다: 조직·공간 구성원으로 들인다(별도 컨텍스트는 준비에만 쓴다).
  const second = await signIn('test-user-2');
  const tag = unique();
  const ws = await api.createWorkspace(`계정 전환 ${tag}`);
  await api.ensureOrganizationMember(second.api, second.user.email);
  await api.invite(ws, second.user.email, 'member');
  await second.api.acceptInvitation(ws);
  const item = await api.createProjectItem(ws, tag);
  // 항목은 기본이 비공개라 공간 구성원도 못 본다. B가 같은 업무를 열 수 있게 공간 공개로 바꾼다.
  const itemView = await api.get(`/api/items/${item}`);
  await api.send('PUT', `/api/items/${item}`, {
    base_version: num(itemView, 'version'),
    title: str(itemView, 'title'),
    description: str(itemView, 'description'),
    visibility: 'workspace',
  });
  const title = `계정 전환 업무 ${tag}`;
  const taskId = str(await api.createTask(item, title), 'id');
  const boardPath = `/w/${ws}/i/${item}/tasks/board`;
  const detailPath = `${boardPath}/t/${taskId}`;
  const edit = `A가 치고 저장하지 않은 설명 ${tag}`;
  const newTitle = `A가 치고 만들지 않은 업무 ${tag}`;
  const a = str(await api.get('/api/me'), 'id');
  const b = str(await second.api.get('/api/me'), 'id');
  const dialog = page.getByRole('dialog', { name: title, exact: true });
  const description = dialog.getByLabel('설명', { exact: true });
  const newTask = page.getByRole('textbox', { name: '새 업무 제목', exact: true });
  const offer = page.getByTestId('task-draft-offer');
  const newOffer = page.getByTestId('task-new-draft-offer');
  const serverTask = async (): Promise<unknown> => api.get(`/api/tasks/${taskId}`);
  // 저장하지 않은 편집의 창 닫기·새로고침 경고(beforeunload)는 떠나기로 답한다. 떠나도 입력은 브라우저에 남아야 한다.
  page.on('dialog', (prompt) => {
    void prompt.accept();
  });

  await test.step('A: 새 업무 제목과 업무 설명을 치고 저장하지 않은 채 새로고침해도 되살린다', async () => {
    await page.goto(boardPath);
    await newTask.fill(newTitle);
    await page.locator(`[data-task-card="${taskId}"]`).click();
    await expect(dialog).toBeVisible();
    await description.fill(edit);
    await expect.poll(() => draftKeys(page, a)).toHaveLength(2);
    await page.reload();
    await expect(dialog).toBeVisible();
    await expect(offer).toBeVisible();
    await expect(description).toHaveValue('');
    await offer.getByRole('button', { name: '입력 복원', exact: true }).click();
    await expect(description).toHaveValue(edit);
    expect(str(await serverTask(), 'description')).toBe('');
    await page.screenshot({ path: testInfo.outputPath('1-a-restored-after-reload.png') });
  });

  const other = await page.context().newPage();
  await test.step('다른 탭에서 B로 바뀐 뒤 창으로 돌아오면 다시 읽히고, B는 A의 입력을 보지 못한다', async () => {
    await first.endTrace();
    expect(await switchTo(other, 'test-user-2')).toBe(b);
    await returnToWindow(page, testInfo);
    // 같은 주소를 B로 다시 읽었다: 업무는 보이지만 A의 입력과 제안은 없다.
    await expect(dialog).toBeVisible();
    expect(str(await (await page.request.get('/api/me')).json(), 'id')).toBe(b);
    await expect(description).toHaveValue('');
    await expect(offer).toHaveCount(0);
    await expect(page.getByText(edit)).toHaveCount(0);
    await expect(page.getByText(newTitle)).toHaveCount(0);
    expect(await draftKeys(page, b)).toEqual([]);
    expect(await draftKeys(page, a)).toHaveLength(2);
    await page.screenshot({ path: testInfo.outputPath('2-b-sees-no-input.png') });
  });

  await test.step('A로 돌아오면 입력을 되살리고, 직접 저장·만들기를 눌러야 서버에 한 번 반영된다', async () => {
    expect(await switchTo(other, 'test-user')).toBe(a);
    await returnToWindow(page, testInfo);
    await expect(dialog).toBeVisible();
    expect(str(await (await page.request.get('/api/me')).json(), 'id')).toBe(a);
    await expect(offer).toBeVisible();
    await expect(description).toHaveValue('');
    expect(str(await serverTask(), 'description')).toBe('');
    await page.screenshot({ path: testInfo.outputPath('3-a-offered-after-return.png') });
    await offer.getByRole('button', { name: '입력 복원', exact: true }).click();
    await expect(description).toHaveValue(edit);
    expect(str(await serverTask(), 'description')).toBe('');
    const save = dialog.getByRole('button', { name: '저장', exact: true });
    await save.click();
    await expect.poll(async () => str(await serverTask(), 'description')).toBe(edit);
    // 남길 때의 판(1) 위에 한 번 저장됐다.
    expect(num(await serverTask(), 'version')).toBe(2);
    await expect(save).toBeFocused();
    await expect(save).toBeEnabled();
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();

    await expect(newOffer).toBeVisible();
    await expect(newOffer).toContainText(newTitle);
    await expect(newTask).toHaveValue('');
    await newOffer.getByRole('button', { name: '입력 복원', exact: true }).click();
    await expect(newTask).toHaveValue(newTitle);
    await expect(newOffer).toHaveCount(0);
    const created = async (): Promise<number> =>
      list(await api.get(`/api/items/${item}/tasks`)).filter((t) => str(t, 'title') === newTitle)
        .length;
    expect(await created()).toBe(0);
    await newTask.press('Enter');
    await expect.poll(created).toBe(1);
    await expect(newTask).toHaveValue('');
    await page.screenshot({ path: testInfo.outputPath('4-a-saved-and-created.png') });
  });

  await test.step('반영된 뒤에는 남은 입력이 없다', async () => {
    await expect.poll(() => draftKeys(page, a)).toEqual([]);
    await page.reload();
    await expect(page.getByTestId('task-board')).toBeVisible();
    await expect(newOffer).toHaveCount(0);
    await page.goto(detailPath);
    await expect(dialog).toBeVisible();
    await expect(description).toHaveValue(edit);
    await expect(offer).toHaveCount(0);
    expect(num(await serverTask(), 'version')).toBe(2);
    expect(
      list(await api.get(`/api/items/${item}/tasks`)).filter((t) => str(t, 'title') === newTitle),
    ).toHaveLength(1);
  });
});
