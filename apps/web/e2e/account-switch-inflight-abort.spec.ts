// #88: 계정 전환을 감지하면 이전 화면은 새 요청을 보내지 않는다(account-switch.spec.ts가 본다). 여기서는 전환을 감지하기 전에
// 이미 나가 있던 요청이 새 계정의 세션 아래에서 끝나지 않는지 본다. 이전 문서가 보낸 업무 읽기 하나를 네트워크에서 붙잡아 두고
// (응답을 흉내 내지 않는다: 보내지도 이어 주지도 않는다) 다른 계정으로 바뀐 창에 돌아오면, ① 다시 읽기로 이전 문서가 내려가는
// 순간(pagehide)에도 그 요청은 아직 진행 중이었고(관찰용 fetch 기록기, 동작은 바꾸지 않음), ② 다시 읽은 뒤 붙잡은 요청을
// 풀어도 응답이 페이지에 오지 않는다. 한계: 붙잡힌 요청에 대해 헤드리스 Chromium은 끊김 이벤트(Playwright requestfailed, CDP
// Network.loadingFailed)를 내지 않아 net::ERR_ABORTED 자체는 관측하지 못한다. 끊김은 ①②로 미루어 보는 것이다.
import { returnToWindow, switchTo } from './account-switch-helpers';
import { expect, list, str, test, unique } from './fixtures';

const PENDING_KEY = 'fvoci-e2e-pending-at-pagehide';

test('a request still in flight when the account switch is detected never completes into the new session', async ({
  signIn,
}, testInfo) => {
  test.setTimeout(180_000);
  const first = await signIn('test-user');
  const { page, api } = first;
  const tag = unique();
  const ws = await api.createWorkspace(`진행 중 요청 ${tag}`);
  const item = await api.createProjectItem(ws, tag);
  const taskId = str(await api.createTask(item, `진행 중 요청 업무 ${tag}`), 'id');
  const a = str(await api.get('/api/me'), 'id');
  const taskPath = `/api/tasks/${taskId}`;
  const isTask = (url: string): boolean => new URL(url).pathname === taskPath;

  const responses: number[] = [];
  page.on('response', (response) => {
    if (isTask(response.url())) responses.push(response.status());
  });

  let held: Parameters<Parameters<typeof page.route>[1]>[0] | undefined;
  await test.step('A: 업무 읽기 요청 하나를 보낸 채 붙잡아 둔다', async () => {
    await page.goto(`/w/${ws}/i/${item}/tasks/board`);
    await expect(page.getByTestId('task-board')).toBeVisible();
    // 진행 중인 /api/ 요청을 적고, 이 문서가 내려갈 때(pagehide) 아직 끝나지 않은 것을 세션 저장소(다시 읽은 뒤에도 남는다)에
    // 남긴다. 요청을 바꾸지 않는 관찰용 기록기다.
    await page.evaluate((key) => {
      const pending = new Map<symbol, string>();
      const original = window.fetch.bind(window);
      window.fetch = (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
        const url =
          typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
        const id = Symbol('fetch');
        pending.set(id, new URL(url, window.location.origin).pathname);
        const request = original(input, init);
        void request.finally(() => pending.delete(id)).catch(() => undefined);
        return request;
      };
      window.addEventListener('pagehide', () => {
        window.sessionStorage.setItem(key, JSON.stringify([...pending.values()]));
      });
    }, PENDING_KEY);
    await page.route(`**${taskPath}`, (route) => {
      // 보내지도 이어 주지도 않는다: 요청은 브라우저 안에서 진행 중으로 남는다.
      held = route;
    });
    const sent = page.waitForRequest(
      (request) => request.method() === 'GET' && isTask(request.url()),
    );
    await page.locator(`[data-task-card="${taskId}"]`).click();
    await sent;
    expect(held, 'request held at the network layer').toBeDefined();
    expect(responses, 'responses before the switch').toEqual([]);
  });

  const other = await page.context().newPage();
  await test.step('B로 바뀐 뒤 창 복귀: 요청은 끝나지 못한 채 이전 문서와 함께 내려가고, 풀어도 응답이 오지 않는다', async () => {
    await first.endTrace();
    expect(await switchTo(other, 'test-user-2')).not.toBe(a);
    await returnToWindow(page, testInfo, (timeout) =>
      page.waitForEvent('load', { timeout }).then(
        () => true,
        () => false,
      ),
    );
    const pending: unknown = JSON.parse(
      (await page.evaluate((key) => window.sessionStorage.getItem(key), PENDING_KEY)) ?? '[]',
    );
    await page.evaluate((key) => {
      window.sessionStorage.removeItem(key);
    }, PENDING_KEY);
    expect(list(pending), 'requests still pending when the previous document went away').toContain(
      taskPath,
    );
    // 다시 읽은 뒤 붙잡았던 요청을 풀어도 새 문서에 응답이 오지 않는다(이미 끊긴 요청).
    await held?.continue();
    await page.waitForTimeout(2_000);
    await page.unroute(`**${taskPath}`);
    expect(responses, 'responses to the in-flight request').toEqual([]);
    expect(str(await (await page.request.get('/api/me')).json(), 'id')).not.toBe(a);
    await page.screenshot({ path: testInfo.outputPath('1-after-reload.png') });
  });
});
