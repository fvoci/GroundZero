// D-96·D-128·#88: 업무 폼(본문·선행 지정)과 새 업무 제목의 저장하지 않은 입력은 같은 브라우저에서 다른 계정으로 바뀌어 다시
// 읽힌 뒤에도 원래 계정에게만 되살린다. 실제 Keycloak 로그아웃·로그인, 실제 창 복귀 확인(FeedbackHost의 /api/me 비교),
// 실제 다시 읽기(window.location.reload)를 거친다. 다시 읽기가 beforeunload에서 취소되거나 네트워크로 실패해도 이전 화면은
// 가려진 채 남고(#88 안전 상태) 이전 화면은 API를 더 부르지 않는다. 다른 계정은 입력을 보지 못하고, 되살린 입력은 사용자가
// 직접 저장·관계 저장·만들기를 눌러야 서버로 간다(중복 저장·생성 없음). 응답을 흉내 내지 않는다. 다시 읽기의 문서 응답을 잠시
// 붙잡아 두는 것(지연 뒤 성공 경로)과 이전 문서 안의 관찰용 기록기(fetch 호출 기록·상태 표본, 동작은 바꾸지 않음)만 쓴다. 실패·취소
// 경로는 account-switch-failed-reload.spec.ts가 본다.
import { appendFileSync } from 'node:fs';

import type { Page, TestInfo } from '@playwright/test';

import {
  expect,
  list,
  listField,
  logIn,
  num,
  recordSensitive,
  str,
  test,
  testUser,
  unique,
} from './fixtures';

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
 * 다른 탭에서 계정을 바꾼 뒤 이 창으로 돌아온다. `arm`은 돌아온 뒤 기대하는 상태를 `timeout` 안에 기다리는 약속을 만든다
 * (이벤트를 놓치지 않게 탭 전환 전에 만든다). 탭 전환(bringToFront)이 focus·visibilitychange를 내지 않는 헤드리스 환경이면
 * focus 이벤트를 직접 낸다. 그 뒤의 확인 요청(/api/me)·계정 비교·안전 상태·다시 읽기는 모두 실제 코드다. 어느 쪽이었는지
 * 기록한다.
 */
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
  const forced = arm(15_000);
  await page.evaluate(() => {
    window.dispatchEvent(new Event('focus'));
  });
  expect(await forced, 'detection after a dispatched focus event').toBe(true);
}

test('unsaved task input survives A→B→A account switches, interrupted reloads and real reloads, for the first account only', async ({
  signIn,
}, testInfo) => {
  test.setTimeout(300_000);
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
  const candidateTitle = `선행 후보 ${tag}`;
  // 날짜 없는 선행 후보: 관계를 저장해도 옮길 후속이 없어 승인 패널이 뜨지 않는다.
  const candidate = str(await api.createTask(item, candidateTitle), 'id');
  const taskId = str(await api.createTask(item, title), 'id');
  const boardPath = `/w/${ws}/i/${item}/tasks/board`;
  const detailPath = `${boardPath}/t/${taskId}`;
  const edit = `A가 치고 저장하지 않은 설명 ${tag}`;
  const newTitle = `A가 치고 만들지 않은 업무 ${tag}`;
  const a = str(await api.get('/api/me'), 'id');
  const b = str(await second.api.get('/api/me'), 'id');
  const dialog = page.getByRole('dialog', { name: title, exact: true });
  const description = dialog.getByLabel('설명', { exact: true });
  const picker = dialog.getByRole('button', { name: '선행 업무 추가', exact: true });
  const lag = dialog.getByLabel('지연(일)', { exact: true });
  const newTask = page.getByRole('textbox', { name: '새 업무 제목', exact: true });
  const offer = page.getByTestId('task-draft-offer');
  const newOffer = page.getByTestId('task-new-draft-offer');
  const serverTask = async (): Promise<unknown> => api.get(`/api/tasks/${taskId}`);
  const predecessorsOf = async (): Promise<{ id: string; lag: number }[]> =>
    listField(await serverTask(), 'predecessors').map((p) => ({
      id: str(p, 'task_id'),
      lag: num(p, 'lag_days'),
    }));
  // 저장하지 않은 편집의 창 닫기·새로고침 경고(beforeunload)는 떠나기로 답한다. 떠나도 입력은 브라우저에 남아야 한다.
  // (헤드리스 Chromium은 스크립트가 부른 다시 읽기에 이 경고를 띄우지 않았다. "취소된 다시 읽기"의 안전 상태는 mock 시험
  // account-safety-c가 보고, 여기서는 다시 읽기의 문서 요청을 붙잡아 둔 "끝나지 않는 다시 읽기"와 끊은 "실패한 다시 읽기"를 본다.)
  page.on('dialog', (prompt) => {
    void prompt.accept();
  });

  await test.step('A: 설명·선행 지정·새 업무 제목을 치고 저장하지 않은 채 새로고침해도 되살린다', async () => {
    await page.goto(boardPath);
    await newTask.fill(newTitle);
    await page.locator(`[data-task-card="${taskId}"]`).click();
    await expect(dialog).toBeVisible();
    await description.fill(edit);
    await picker.click();
    await page.getByRole('option', { name: new RegExp(candidateTitle) }).click();
    await lag.fill('2');
    await expect.poll(() => draftKeys(page, a)).toHaveLength(2);
    await page.reload();
    await expect(dialog).toBeVisible();
    await expect(offer).toBeVisible();
    await expect(description).toHaveValue('');
    await offer.getByRole('button', { name: '입력 복원', exact: true }).click();
    await expect(description).toHaveValue(edit);
    await expect(lag).toHaveValue('2');
    await expect(picker).toContainText(candidateTitle);
    expect(str(await serverTask(), 'description')).toBe('');
    expect(await predecessorsOf()).toEqual([]);
    await page.screenshot({ path: testInfo.outputPath('1-a-restored-after-reload.png') });
  });

  const other = await page.context().newPage();
  await test.step('B로 바뀐 뒤 창 복귀: 다시 읽기가 끝나지 않는 동안 이전 화면은 안전 상태로 가려지고 API를 부르지 않는다', async () => {
    await first.endTrace();
    expect(await switchTo(other, 'test-user-2')).toBe(b);
    // 다시 읽기의 문서 응답을 2.5초 붙잡아 둔다(느린 다시 읽기). 그동안 이전 문서가 살아 있다. 이동이 걸려 있는 동안은 Playwright
    // 조작이 막히므로, 이전 문서 안의 기록기가 안전 상태를 0.1초마다 표본으로 남기고(세션 저장소, 같은 탭의 다시 읽기 뒤에도
    // 남는다) 복귀 이벤트를 계속 내서 이전 화면이 요청을 보내는지 네트워크에서 본다.
    const HOLD_MS = 2_500;
    const traffic: { t: number; kind: string; path: string }[] = [];
    page.on('request', (request) => {
      traffic.push({
        t: Date.now(),
        kind: request.resourceType(),
        path: new URL(request.url()).pathname,
      });
    });
    await page.route('**/*', async (route) => {
      if (route.request().resourceType() === 'document')
        await new Promise((resolve) => setTimeout(resolve, HOLD_MS));
      await route.continue();
    });
    await page.evaluate(() => {
      // 이전 문서가 보내는 API 호출을 호출 시점의 전환 상태와 함께 적는다(네트워크 이벤트의 시각 순서는 수 ms 어긋날 수 있다).
      const fetches: { path: string; switching: boolean }[] = [];
      const original = window.fetch.bind(window);
      window.fetch = (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
        const url =
          typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
        if (url.includes('/api/')) {
          fetches.push({
            path: new URL(url, window.location.origin).pathname,
            switching: document.documentElement.hasAttribute('data-account-switching'),
          });
          window.sessionStorage.setItem('fvoci-e2e-fetches', JSON.stringify(fetches));
        }
        return original(input, init);
      };
      const start = Date.now();
      const samples: {
        t: number;
        switching: boolean;
        safeVisible: boolean;
        descriptionVisible: boolean;
      }[] = [];
      const visible = (el: Element | null): boolean =>
        el !== null &&
        getComputedStyle(el).visibility === 'visible' &&
        el.getClientRects().length > 0;
      window.setInterval(() => {
        const safe = document.querySelector('[data-testid="account-safe-state"]');
        const box = document.querySelector('[role="dialog"] textarea');
        samples.push({
          t: Date.now() - start,
          switching: document.documentElement.hasAttribute('data-account-switching'),
          safeVisible: visible(safe),
          descriptionVisible: visible(box),
        });
        window.sessionStorage.setItem('fvoci-e2e-probe', JSON.stringify(samples.slice(-60)));
        // 전환이 켜진 뒤에만 복귀 이벤트를 더 낸다(감지 자체는 탭 전환 또는 시험이 낸 focus가 하고, 그 뒤에는 확인 요청조차
        // 나가지 않아야 한다).
        if (document.documentElement.hasAttribute('data-account-switching')) {
          window.dispatchEvent(new Event('focus'));
          document.dispatchEvent(new Event('visibilitychange'));
        }
      }, 100);
    });
    await returnToWindow(page, testInfo, (timeout) =>
      page.waitForEvent('load', { timeout }).then(
        () => true,
        () => false,
      ),
    );
    await page.unroute('**/*');
    // 같은 주소를 B로 다시 읽었다. 이전 문서가 남긴 표본: 전환이 켜진 뒤 안전 상태가 보이고 설명 칸은 숨겨졌다.
    const samples: unknown = JSON.parse(
      (await page.evaluate(() => window.sessionStorage.getItem('fvoci-e2e-probe'))) ?? '[]',
    );
    await page.evaluate(() => {
      window.sessionStorage.removeItem('fvoci-e2e-probe');
    });
    const covered = list(samples).filter(
      (s) =>
        typeof s === 'object' &&
        s !== null &&
        'switching' in s &&
        s.switching === true &&
        'safeVisible' in s &&
        s.safeVisible === true &&
        'descriptionVisible' in s &&
        s.descriptionVisible === false,
    );
    expect(covered.length, 'samples with the previous screen covered').toBeGreaterThanOrEqual(10);
    // 이전 화면은 다시 읽기가 걸려 있는 동안 API를 부르지 않았다(확인 요청 /api/me 한 번 뒤 문서 요청, 그 뒤로는 없음). 확인 요청이
    // 돌아오기 전의 복귀 갱신(타이머·안 읽은 수 읽기)은 아직 전환을 모르는 상태라 나갈 수 있다(⑫의 남는 위험과 같은 창, 읽기뿐).
    const reloadDocument = traffic.find((r) => r.kind === 'document');
    if (reloadDocument === undefined) throw new Error('no reload document request recorded');
    const detection = traffic.filter((r) => r.path === '/api/me' && r.t <= reloadDocument.t);
    expect(detection.length, 'one /api/me check before the reload').toBe(1);
    // 이전 문서가 전환이 켜진 뒤에 보낸 API 호출은 없어야 한다(호출 시점의 전환 상태로 판정; 네트워크 이벤트의 시각 순서는
    // 수 ms 어긋날 수 있어 쓰지 않는다). 확인 요청이 돌아오기 전의 복귀 갱신(타이머·안 읽은 수 읽기)은 전환을 모르는 상태라
    // 나갈 수 있다(⑫의 남는 위험과 같은 창, 읽기뿐).
    const fetches: unknown = JSON.parse(
      (await page.evaluate(() => window.sessionStorage.getItem('fvoci-e2e-fetches'))) ?? '[]',
    );
    await page.evaluate(() => {
      window.sessionStorage.removeItem('fvoci-e2e-fetches');
    });
    const leaked = list(fetches).filter(
      (f) => typeof f === 'object' && f !== null && 'switching' in f && f.switching === true,
    );
    expect(leaked, 'API calls from the previous screen after the switch').toEqual([]);
    expect(list(fetches).length, 'API calls recorded by the previous screen').toBeGreaterThan(0);
    page.removeAllListeners('request');
    await page.screenshot({ path: testInfo.outputPath('2-b-after-stalled-reload.png') });
    // 같은 주소를 B로 읽었다: 업무는 보이지만 A의 입력·제안은 없다(선행 지정 칸도 기본값).
    await expect(dialog).toBeVisible();
    expect(str(await (await page.request.get('/api/me')).json(), 'id')).toBe(b);
    await expect(description).toHaveValue('');
    await expect(lag).toHaveValue('0');
    await expect(picker).toContainText('업무 고르기');
    await expect(offer).toHaveCount(0);
    await expect(page.getByText(edit)).toHaveCount(0);
    await expect(page.getByText(newTitle)).toHaveCount(0);
    expect(await draftKeys(page, b)).toEqual([]);
    expect(await draftKeys(page, a)).toHaveLength(2);
    await page.screenshot({ path: testInfo.outputPath('3-b-sees-no-input.png') });
  });

  await test.step('A로 돌아오면 입력을 되살리고, 직접 저장·관계 저장·만들기를 눌러야 서버에 한 번 반영된다', async () => {
    expect(await switchTo(other, 'test-user')).toBe(a);
    await returnToWindow(page, testInfo, (timeout) =>
      page.waitForEvent('load', { timeout }).then(
        () => true,
        () => false,
      ),
    );
    await expect(dialog).toBeVisible();
    expect(str(await (await page.request.get('/api/me')).json(), 'id')).toBe(a);
    await expect(offer).toBeVisible();
    await expect(description).toHaveValue('');
    await page.screenshot({ path: testInfo.outputPath('4-a-offered-after-return.png') });
    await offer.getByRole('button', { name: '입력 복원', exact: true }).click();
    await expect(description).toHaveValue(edit);
    await expect(lag).toHaveValue('2');
    await expect(picker).toContainText(candidateTitle);
    expect(str(await serverTask(), 'description')).toBe('');
    expect(await predecessorsOf()).toEqual([]);

    const save = dialog.getByRole('button', { name: '저장', exact: true });
    await save.click();
    await expect.poll(async () => str(await serverTask(), 'description')).toBe(edit);
    // 남길 때의 판(1) 위에 한 번 저장됐다.
    expect(num(await serverTask(), 'version')).toBe(2);
    await expect(save).toBeFocused();
    await expect(save).toBeEnabled();
    // 선행 지정은 "관계 저장"을 눌러야 가고, 한 번만 반영된다.
    await dialog.getByRole('button', { name: '관계 저장', exact: true }).click();
    await expect.poll(predecessorsOf).toEqual([{ id: candidate, lag: 2 }]);
    // 저장된 관계는 목록에 보이고, 남은 후보가 없어 새 관계 입력 칸은 닫힌다.
    await expect(
      dialog.getByRole('button', { name: new RegExp(`${candidateTitle} 관계 고치기`) }),
    ).toBeVisible();
    await expect(lag).toHaveCount(0);
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
    await page.screenshot({ path: testInfo.outputPath('5-a-saved-linked-created.png') });
  });

  await test.step('반영된 뒤에는 남은 입력이 없고 중복도 없다', async () => {
    await expect.poll(() => draftKeys(page, a)).toEqual([]);
    await page.reload();
    await expect(page.getByTestId('task-board')).toBeVisible();
    await expect(newOffer).toHaveCount(0);
    await page.goto(detailPath);
    await expect(dialog).toBeVisible();
    await expect(description).toHaveValue(edit);
    await expect(offer).toHaveCount(0);
    expect(await predecessorsOf()).toEqual([{ id: candidate, lag: 2 }]);
    expect(
      list(await api.get(`/api/items/${item}/tasks`)).filter((t) => str(t, 'title') === newTitle),
    ).toHaveLength(1);
  });
});
