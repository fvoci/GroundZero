import { afterEach, expect, it, vi } from 'vitest';

import { PRELOAD_RELOAD_KEY, PRELOAD_RELOAD_SETTLE_MS } from '$lib/features/common/preload-reload';

// Storybook 미리보기(.storybook/preview.ts)가 옛 청크를 못 읽을 때(`vite:preloadError`) 실패 묶음마다 한 번 다시 읽는
// 처리를 거는지. preview.ts는 가져오는 순간 fetch를 바꾸고 이 처리를 창에 걸어 두므로 다른 시험과 섞이지 않게 파일을 따로
// 둔다. 처리의 세부는 preload-reload.test.ts가 본다.
//
// 보고된 경우: 브랜치 미리보기는 push마다 같은 주소를 덮어써, 오래 연 탭이 배포를 여러 번 만난다. 한 번 다시 읽은 뒤에도
// 다음 배포에서 또 한 번 다시 읽어야 "Importing a module script failed"가 다시 나지 않는다.

afterEach(() => {
  vi.useRealTimers();
  sessionStorage.removeItem(PRELOAD_RELOAD_KEY);
});

function preloadError(): boolean {
  const event = new Event('vite:preloadError', { cancelable: true });
  window.dispatchEvent(event);
  return event.defaultPrevented;
}

it('the Storybook preview reloads again for a later deploy, but not for the same failure', async () => {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
  // 방금 다시 읽어 뜬 미리보기(앞 실패 때 쓴 표지가 있다).
  sessionStorage.setItem(PRELOAD_RELOAD_KEY, '2026-10-27T00:00:00.000Z');
  await import('../../.storybook/preview');

  // 뜬 직후 또 나면 같은 실패로 보고 다시 읽지 않는다(오류는 그대로 둔다, 무한 다시 읽기 방지).
  expect(preloadError()).toBe(false);

  // 오래 열어 둔 뒤 다음 배포를 만나면 다시 한 번 다시 읽는다. jsdom은 실제로 다시 읽지 않으므로(navigation 미구현,
  // 로그에 "Not implemented: navigation") 취소와 새 표지로 다시 읽기 경로를 탔는지 본다.
  vi.advanceTimersByTime(PRELOAD_RELOAD_SETTLE_MS);
  expect(sessionStorage.getItem(PRELOAD_RELOAD_KEY)).toBeNull();
  expect(preloadError()).toBe(true);
  expect(sessionStorage.getItem(PRELOAD_RELOAD_KEY)).not.toBeNull();
});
