// #88: 계정 전환 감지 뒤의 다시 읽기는 "지연 뒤 성공"(account-switch.spec.ts), "실패"(이 파일: 연결 끊김·서버 5xx), "사용자 취소"
// (beforeunload)가 서로 다른 경로다. 여기서는 다시 읽기의 문서 요청을 실제로 실패시킨다. 실패하면 브라우저 오류 화면이 되어 이전
// 화면·입력은 보이지 않고, 실패하는 동안 이전 화면은 API 쓰기를 보내지 않으며, 입력은 원래 계정의 브라우저 초안으로 남아 그 계정이
// 돌아와 확인하면 되살려 직접 다시 저장한다. 다른 계정은 그 입력을 보지 못한다. 취소 경로는 헤드리스 Chromium이 스크립트 다시
// 읽기에 beforeunload 경고를 띄우는지 기록하고, 띄울 수 있는 환경에서만 취소 뒤 안전 상태 유지를 단언한다(도구 제약을 제품
// 통과로 바꾸지 않는다). 응답을 흉내 내지 않는다(실패시킬 뿐).
import { appendFileSync } from 'node:fs';

import type { Page, TestInfo } from '@playwright/test';

import { expect, logIn, num, recordSensitive, str, test, testUser, unique } from './fixtures';

async function draftKeys(page: Page, userId: string): Promise<string[]> {
  return page.evaluate(
    (prefix) => Object.keys(window.localStorage).filter((k) => k.startsWith(prefix)),
    `fvoci:${userId}:-:form-draft:`,
  );
}

async function switchTo(page: Page, username: 'test-user' | 'test-user-2'): Promise<string> {
  await page.goto('/');
  await page.getByTestId('user-menu').click();
  await page.getByRole('menuitem', { name: '로그아웃' }).click();
  await expect(page.getByRole('link', { name: '로그인' })).toBeVisible();
  await logIn(page, testUser(username));
  recordSensitive((await page.context().cookies()).map((c) => c.value));
  return str(await (await page.request.get('/api/me')).json(), 'id');
}

/** 창으로 돌아온다. 헤드리스는 탭 전환이 감지 이벤트를 내지 않으므로 focus를 직접 낸다(그 뒤의 확인·전환·다시 읽기는 실제 코드). */
async function returnToWindow(
  page: Page,
  testInfo: TestInfo,
  arm: (timeout: number) => Promise<boolean>,
): Promise<void> {
  const note = (text: string): void => {
    testInfo.annotations.push({ type: 'focus', description: text });
    appendFileSync(testInfo.outputPath('focus-path.txt'), `${text}\n`);
  };
  const natural = arm(5_000);
  await page.bringToFront();
  if (await natural) {
    note('bringToFront fired a detection event (focus or visibilitychange)');
    return;
  }
  note('headless: window focus dispatched programmatically (not a real tab switch)');
  const forced = arm(20_000);
  await page.evaluate(() => {
    window.dispatchEvent(new Event('focus'));
  });
  expect(await forced, 'detection after a dispatched focus event').toBe(true);
}

test('a failed reload after an account switch leaves no previous screen, writes nothing, and only the first account recovers the input', async ({
  signIn,
}, testInfo) => {
  test.setTimeout(300_000);
  const first = await signIn('test-user');
  const { page, api } = first;
  const second = await signIn('test-user-2');
  const tag = unique();
  const ws = await api.createWorkspace(`다시 읽기 실패 ${tag}`);
  await api.ensureOrganizationMember(second.api, second.user.email);
  await api.invite(ws, second.user.email, 'member');
  await second.api.acceptInvitation(ws);
  const item = await api.createProjectItem(ws, tag);
  const itemView = await api.get(`/api/items/${item}`);
  await api.send('PUT', `/api/items/${item}`, {
    base_version: num(itemView, 'version'),
    title: str(itemView, 'title'),
    description: str(itemView, 'description'),
    visibility: 'workspace',
  });
  const title = `다시 읽기 실패 업무 ${tag}`;
  const taskId = str(await api.createTask(item, title), 'id');
  const boardPath = `/w/${ws}/i/${item}/tasks/board`;
  const detailPath = `${boardPath}/t/${taskId}`;
  const edit = `A가 치고 저장하지 않은 설명 ${tag}`;
  const a = str(await api.get('/api/me'), 'id');
  const b = str(await second.api.get('/api/me'), 'id');
  const dialog = page.getByRole('dialog', { name: title, exact: true });
  const description = dialog.getByLabel('설명', { exact: true });
  const offer = page.getByTestId('task-draft-offer');
  const serverTask = async (): Promise<unknown> => api.get(`/api/tasks/${taskId}`);
  // 실패 중 이전 화면이 보내는 쓰기 요청(GET이 아닌 /api/ 호출)을 모두 적는다.
  const writes: string[] = [];
  page.on('request', (request) => {
    if (request.url().includes('/api/') && request.method() !== 'GET')
      writes.push(`${request.method()} ${new URL(request.url()).pathname}`);
  });
  page.on('dialog', (prompt) => {
    void prompt.accept();
  });

  await test.step('A: 설명을 치고 저장하지 않는다', async () => {
    await page.goto(detailPath);
    await expect(dialog).toBeVisible();
    await description.fill(edit);
    await expect.poll(() => draftKeys(page, a)).toHaveLength(1);
    expect(str(await serverTask(), 'description')).toBe('');
  });

  const other = await page.context().newPage();
  await test.step('B로 바뀐 뒤 다시 읽기가 연결 끊김으로 실패한다: 이전 화면은 남지 않고 쓰기도 없다', async () => {
    await first.endTrace();
    expect(await switchTo(other, 'test-user-2')).toBe(b);
    await page.route('**/*', async (route) => {
      if (route.request().resourceType() === 'document') await route.abort('internetdisconnected');
      else await route.continue();
    });
    writes.length = 0;
    await returnToWindow(page, testInfo, (timeout) =>
      expect(description)
        .toHaveCount(0, { timeout })
        .then(
          () => true,
          () => false,
        ),
    );
    // 브라우저 오류 화면: 앱도, A의 입력도 보이지 않는다.
    await expect(page.getByTestId('task-board')).toHaveCount(0);
    await expect(page.getByText(edit)).toHaveCount(0);
    await page.waitForTimeout(1_000);
    expect(writes, 'writes from the previous screen while the reload failed').toEqual([]);
    await page.screenshot({ path: testInfo.outputPath('1-failed-reload-disconnected.png') });
    await page.unroute('**/*');
    // 연결이 돌아와 B로 읽으면 B의 화면이다: A의 입력·제안은 없고 A의 초안은 A의 키에 남아 있다.
    await page.goto(detailPath);
    await expect(dialog).toBeVisible();
    expect(str(await (await page.request.get('/api/me')).json(), 'id')).toBe(b);
    await expect(description).toHaveValue('');
    await expect(offer).toHaveCount(0);
    await expect(page.getByText(edit)).toHaveCount(0);
    expect(await draftKeys(page, b)).toEqual([]);
    expect(await draftKeys(page, a)).toHaveLength(1);
    await page.screenshot({ path: testInfo.outputPath('2-b-after-disconnected.png') });
  });

  await test.step('A로 바뀐 뒤 다시 읽기가 서버 오류(503)로 실패한다: 오류 화면에는 아무 화면도 없고, 다시 읽으면 A가 입력을 되살려 직접 저장한다', async () => {
    expect(await switchTo(other, 'test-user')).toBe(a);
    await page.route('**/*', async (route) => {
      if (route.request().resourceType() === 'document')
        await route.fulfill({
          status: 503,
          contentType: 'text/plain',
          body: 'service unavailable (e2e)',
        });
      else await route.continue();
    });
    writes.length = 0;
    await returnToWindow(page, testInfo, (timeout) =>
      expect(description)
        .toHaveCount(0, { timeout })
        .then(
          () => true,
          () => false,
        ),
    );
    await expect(page.getByText('service unavailable (e2e)')).toBeVisible();
    await expect(page.getByTestId('task-board')).toHaveCount(0);
    await page.waitForTimeout(1_000);
    expect(writes, 'writes while the reload answered 503').toEqual([]);
    await page.screenshot({ path: testInfo.outputPath('3-failed-reload-503.png') });
    await page.unroute('**/*');
    await page.goto(detailPath);
    await expect(dialog).toBeVisible();
    expect(str(await (await page.request.get('/api/me')).json(), 'id')).toBe(a);
    await expect(offer).toBeVisible();
    await expect(description).toHaveValue('');
    expect(str(await serverTask(), 'description')).toBe('');
    await offer.getByRole('button', { name: '입력 복원', exact: true }).click();
    await expect(description).toHaveValue(edit);
    expect(str(await serverTask(), 'description')).toBe('');
    const save = dialog.getByRole('button', { name: '저장', exact: true });
    await save.click();
    await expect.poll(async () => str(await serverTask(), 'description')).toBe(edit);
    expect(num(await serverTask(), 'version')).toBe(2);
    await expect(save).toBeFocused();
    await expect(save).toBeEnabled();
    await expect.poll(() => draftKeys(page, a)).toEqual([]);
    await page.screenshot({ path: testInfo.outputPath('4-a-recovered-after-503.png') });
  });

  await test.step('취소 경로 기록: 저장하지 않은 편집에서 스크립트가 부른 다시 읽기에 beforeunload 경고가 뜨는지', async () => {
    // 지원 경로: Playwright dialog 이벤트(헤드리스 Chromium). 경고가 뜨면 머무르기로 답해 안전 상태·입력이 남는지 보고, 뜨지
    // 않으면 그 사실을 기록한다(이 환경에서는 실제 취소 UI를 만들 수 없다 → 실제 브라우저 NOT RUN, mock account-safety-c만).
    const seen: string[] = [];
    page.removeAllListeners('dialog');
    page.on('dialog', (prompt) => {
      seen.push(prompt.type());
      void prompt.dismiss();
    });
    await description.fill(`${edit} (다시 편집)`);
    await expect.poll(() => draftKeys(page, a)).toHaveLength(1);
    const reloaded = page.waitForEvent('load', { timeout: 5_000 }).then(
      () => true,
      () => false,
    );
    await page.evaluate(() => {
      window.location.reload();
    });
    const didReload = await reloaded;
    const note = `beforeunload dialog seen: ${seen.join(',') || 'none'}; reload happened: ${String(didReload)}`;
    testInfo.annotations.push({ type: 'beforeunload', description: note });
    appendFileSync(testInfo.outputPath('beforeunload-dialog.txt'), `${note}\n`);
    if (seen.includes('beforeunload')) {
      // 취소된 다시 읽기: 화면과 입력이 그대로다.
      await expect(description).toHaveValue(`${edit} (다시 편집)`);
    } else {
      // 경고 없이 다시 읽혔다: 입력은 초안으로 남아 되살릴 수 있다.
      await expect(dialog).toBeVisible();
      await expect(offer).toBeVisible();
    }
  });
});
