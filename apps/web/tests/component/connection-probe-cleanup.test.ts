import { afterAll, afterEach, expect, it, vi } from 'vitest';

import { connection, reportConnectionLost } from '$lib/features/feedback/feedback.svelte';

// #309 곁가지: 연결 확인(`/healthz`를 5초마다)은 앱 수명 동안 도는 모듈 수준 타이머다. 제품에서는 의도된 동작이지만,
// 시험이 연결 끊김을 알리고 끝나면 그 타이머가 같은 파일의 뒤 시험까지 남아 실제 요청을 보냈다. 시험 정리 단계에서
// 멈춰야 한다. 두 시험은 순서대로 돈다: 앞 시험이 확인을 켜고 끝나고, 뒤 시험이 그 확인이 남지 않았는지 본다.
// 같은 가짜 시계를 두 시험에 걸쳐 두어, 남은 타이머가 있으면 뒤 시험에서 발화하게 한다.

afterEach(() => {
  connection.lost = false;
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

afterAll(() => {
  vi.useRealTimers();
});

function healthChecks(fetch: ReturnType<typeof vi.fn>): number {
  return fetch.mock.calls.filter(([url]) => url === '/healthz').length;
}

it('starts the connection check when the connection is lost and ends', () => {
  vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
  reportConnectionLost();
  expect(connection.lost).toBe(true);
});

it('does not run the previous test connection check, and starts its own at the same pace', async () => {
  const fetch = vi.fn(() => Promise.resolve(new Response('', { status: 503 })));
  vi.stubGlobal('fetch', fetch);
  await vi.advanceTimersByTimeAsync(5_000);
  expect(healthChecks(fetch)).toBe(0);
  // 제품 동작은 그대로다: 이 시험에서 끊기면 5초 뒤 확인한다.
  reportConnectionLost();
  await vi.advanceTimersByTimeAsync(4_999);
  expect(healthChecks(fetch)).toBe(0);
  await vi.advanceTimersByTimeAsync(1);
  expect(healthChecks(fetch)).toBe(1);
  expect(fetch).toHaveBeenLastCalledWith('/healthz', { cache: 'no-store' });
});
