import { expect, it } from 'vitest';

import { PRELOAD_RELOAD_KEY } from '$lib/features/common/preload-reload';

// Storybook 미리보기(.storybook/preview.ts)가 옛 청크를 못 읽을 때(`vite:preloadError`) 한 번만 다시 읽는 처리를 거는지.
// preview.ts는 가져오는 순간 fetch를 바꾸고 이 처리를 창에 걸어 두므로 다른 시험과 섞이지 않게 파일을 따로 둔다. 처리의
// 세부(1회 제한·취소하지 않는 경우)는 preload-reload.test.ts가 본다.
it('the Storybook preview installs the reload-once handler', async () => {
  sessionStorage.removeItem(PRELOAD_RELOAD_KEY);
  await import('../../.storybook/preview');
  const event = new Event('vite:preloadError', { cancelable: true });
  window.dispatchEvent(event);
  // jsdom은 실제로 다시 읽지 않는다(navigation 미구현, 로그에 "Not implemented: navigation"). 취소와 이 탭의 표지로 다시
  // 읽기 경로를 탔는지 본다.
  expect(event.defaultPrevented).toBe(true);
  expect(sessionStorage.getItem(PRELOAD_RELOAD_KEY)).not.toBeNull();
  sessionStorage.removeItem(PRELOAD_RELOAD_KEY);
});
