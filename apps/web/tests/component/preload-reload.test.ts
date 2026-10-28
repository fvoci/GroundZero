import { homeApi } from '$lib/features/home/api';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import { leavingWithDrafts, stopKeepingDrafts } from '$lib/features/common/leave-guard.svelte';
import {
  PRELOAD_RELOAD_KEY,
  PRELOAD_RELOAD_SETTLE_MS,
  PRELOAD_RELOAD_STALL_MS,
  watchPreloadErrors,
} from '$lib/features/common/preload-reload';
import { newVersion } from '$lib/features/feedback/feedback.svelte';
import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
import { watchNewVersion } from '$lib/features/feedback/new-version';
import HomeDashboard from '$lib/features/home/HomeDashboard.svelte';
import { homeView } from '../../stories/fixtures/synthetic';
import { homeExtras } from './fixtures/home';
import { org } from '$lib/org.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

// 새 버전 배포 뒤 옛 청크를 못 읽을 때(`vite:preloadError`): 실패 묶음마다 한 번 다시 읽고(취소), 다시 읽어 뜬 직후 또 나거나 미저장
// 입력을 초안으로 남기지 못하면 다시 읽지 않고 배너를 띄운다(D-27·D-96·D-82). Storybook 미리보기는
// preload-reload-storybook.test.ts.

/** sessionStorage를 대신하는 저장소(쓰기 실패를 흉내 낼 수 있다). */
function memoryStorage(failing = false): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear: () => {
      values.clear();
    },
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => {
      values.delete(key);
    },
    setItem: (key, value) => {
      if (failing) throw new DOMException('quota', 'QuotaExceededError');
      values.set(key, value);
    },
  };
}

/** Vite처럼 취소 가능한 이벤트를 보내고 취소됐는지 돌려준다. */
function preloadError(): boolean {
  const event = new Event('vite:preloadError', { cancelable: true });
  window.dispatchEvent(event);
  return event.defaultPrevented;
}

let stop: (() => void) | undefined;
afterEach(() => {
  stop?.();
  stop = undefined;
  stopKeepingDrafts();
  newVersion.times = 0;
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  me.userId = '';
});

describe('watchPreloadErrors', () => {
  it('cancels the first failure and reloads, writing the marker', () => {
    const storage = memoryStorage();
    const reload = vi.fn();
    const onStale = vi.fn();
    stop = watchPreloadErrors({ storage: () => storage, reload, onStale });
    expect(preloadError()).toBe(true);
    expect(reload).toHaveBeenCalledTimes(1);
    expect(storage.getItem(PRELOAD_RELOAD_KEY)).not.toBeNull();
    // 한 import가 여러 번 보내도 다시 읽기는 한 번이고 모두 취소한다.
    expect(preloadError()).toBe(true);
    expect(reload).toHaveBeenCalledTimes(1);
    expect(onStale).not.toHaveBeenCalled();
  });

  it('does not reload again for a failure soon after the reloaded page started', () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    const storage = memoryStorage();
    // 표지를 쓴 때는 오래전이어도(느린 회선) 시간은 새 페이지가 뜬 때부터 잰다.
    storage.setItem(PRELOAD_RELOAD_KEY, '2026-10-27T00:00:00.000Z');
    const reload = vi.fn();
    const onStale = vi.fn();
    stop = watchPreloadErrors({ storage: () => storage, reload, onStale });
    vi.advanceTimersByTime(PRELOAD_RELOAD_SETTLE_MS - 1);
    // 같은 실패 묶음: 취소하지 않아 오류가 import한 곳에 그대로 간다.
    expect(preloadError()).toBe(false);
    expect(reload).not.toHaveBeenCalled();
    expect(onStale).toHaveBeenCalledTimes(1);
  });

  it('reloads once more for a later deploy after the reloaded page stayed up', () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    const storage = memoryStorage();
    storage.setItem(PRELOAD_RELOAD_KEY, '2026-10-27T00:00:00.000Z');
    const reload = vi.fn();
    const onStale = vi.fn();
    stop = watchPreloadErrors({ storage: () => storage, reload, onStale });
    vi.advanceTimersByTime(PRELOAD_RELOAD_SETTLE_MS);
    expect(storage.getItem(PRELOAD_RELOAD_KEY)).toBeNull();
    expect(preloadError()).toBe(true);
    expect(reload).toHaveBeenCalledTimes(1);
    expect(onStale).not.toHaveBeenCalled();
  });

  it('does not reload when the marker cannot be read', () => {
    const reload = vi.fn();
    const onStale = vi.fn();
    const storage = memoryStorage();
    storage.getItem = () => {
      throw new DOMException('denied', 'SecurityError');
    };
    stop = watchPreloadErrors({ storage: () => storage, reload, onStale });
    expect(preloadError()).toBe(false);
    expect(reload).not.toHaveBeenCalled();
    expect(onStale).toHaveBeenCalledTimes(1);
  });

  it('does not reload when the marker cannot be written', () => {
    const reload = vi.fn();
    const onStale = vi.fn();
    const storage = memoryStorage(true);
    stop = watchPreloadErrors({ storage: () => storage, reload, onStale });
    expect(preloadError()).toBe(false);
    expect(reload).not.toHaveBeenCalled();
    expect(onStale).toHaveBeenCalledTimes(1);
  });

  it('does not reload when not ready, and leaves the marker unset for a later try', () => {
    const storage = memoryStorage();
    const reload = vi.fn();
    const onStale = vi.fn();
    stop = watchPreloadErrors({ storage: () => storage, reload, onStale, ready: () => false });
    expect(preloadError()).toBe(false);
    expect(reload).not.toHaveBeenCalled();
    expect(onStale).toHaveBeenCalledTimes(1);
    expect(storage.getItem(PRELOAD_RELOAD_KEY)).toBeNull();
  });

  // R5 B 최종 C-1: 다시 읽기를 바깥 beforeunload(확장 등)가 막아 사용자가 머무르면, 다시 읽는 중 상태가 영구히 남아 이후
  // 실패를 모두 조용히 취소한다(import가 `undefined`로 끝남). 정해진 시간 안에 페이지가 내려가지 않으면 되돌린다.
  it('undoes a reload that did not happen, and leaves later failures uncancelled', () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    const storage = memoryStorage();
    const reload = vi.fn();
    const onStale = vi.fn();
    const cancelReload = vi.fn();
    stop = watchPreloadErrors({ storage: () => storage, reload, onStale, cancelReload });
    expect(preloadError()).toBe(true);
    expect(reload).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(PRELOAD_RELOAD_STALL_MS);
    expect(cancelReload).toHaveBeenCalledTimes(1);
    expect(onStale).toHaveBeenCalledTimes(1);
    // 같은 실패 묶음이라 다시 읽지 않고, 오류는 취소하지 않는다.
    expect(preloadError()).toBe(false);
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('leaves a reload that is unloading the page alone', () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    const storage = memoryStorage();
    const reload = vi.fn(() => {
      window.dispatchEvent(new Event('pagehide'));
    });
    const onStale = vi.fn();
    const cancelReload = vi.fn();
    stop = watchPreloadErrors({ storage: () => storage, reload, onStale, cancelReload });
    expect(preloadError()).toBe(true);
    vi.advanceTimersByTime(PRELOAD_RELOAD_STALL_MS);
    expect(cancelReload).not.toHaveBeenCalled();
    expect(onStale).not.toHaveBeenCalled();
  });

  it('stops listening after cleanup', () => {
    const reload = vi.fn();
    const storage = memoryStorage();
    watchPreloadErrors({ storage: () => storage, reload })();
    expect(preloadError()).toBe(false);
    expect(reload).not.toHaveBeenCalled();
  });
});

describe('app: watchNewVersion keeps drafts before reloading', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    localStorage.clear();
    me.userId = 'user-a';
    vi.spyOn(api, 'me').mockResolvedValue(undefined);
    // FeedbackHost가 좁은 화면인지 본다(jsdom에는 matchMedia가 없다).
    vi.stubGlobal('matchMedia', (query: string) => ({
      matches: false,
      media: query,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
    }));
  });

  /** 새 업무 제목은 배포 전 영구 초안으로 남긴다(D-27·#154). */
  async function dirtyTaskTitle(): Promise<void> {
    homeExtras();
    const home = homeView();
    const item = home.active_items.find((value) => value.can_write);
    if (item === undefined) throw new Error('missing writable item');
    localStorage.setItem(org.key('quick-add-item'), item.id);
    vi.spyOn(homeApi, 'home').mockResolvedValue(home);
    vi.spyOn(api, 'notifications').mockResolvedValue({ notifications: [], next: null });
    render(HomeDashboard, { workspaces: [] });
    await fireEvent.click(await screen.findByRole('button', { name: m.home_quick_add() }));
    await fireEvent.input(await screen.findByLabelText(m.home_title()), {
      target: { value: '배포 중 입력' },
    });
  }

  const draftKeys = (): string[] =>
    Object.keys(localStorage).filter((key) => key.includes('form-draft:home-quick-task:'));

  it('keeps a dirty form as a draft, then reloads without a leave prompt', async () => {
    await dirtyTaskTitle();
    localStorage.clear();
    let keptBeforeReload = 0;
    const reload = vi.fn(() => {
      // 다시 읽기 전에(beforeunload보다 먼저) 남기기가 이미 초안을 썼다.
      keptBeforeReload = draftKeys().length;
      // 다시 읽을 때의 beforeunload가 남긴 입력으로 막지 않는다.
      const unload = new Event('beforeunload', { cancelable: true });
      window.dispatchEvent(unload);
      expect(unload.defaultPrevented).toBe(false);
    });
    const storage = memoryStorage();
    stop = watchNewVersion({ storage: () => storage, reload });
    expect(preloadError()).toBe(true);
    expect(reload).toHaveBeenCalledTimes(1);
    // 다시 읽기 직전에 남기기가 이 입력을 초안으로 썼다.
    expect(keptBeforeReload).toBe(1);
    expect(draftKeys()).toHaveLength(1);
    expect(localStorage.getItem(draftKeys()[0] ?? '')).toContain('배포 중 입력');
    expect(newVersion.times).toBe(0);
  });

  it('shows the banner and does not reload when a draft cannot be kept', async () => {
    render(FeedbackHost, { reload: vi.fn() });
    await dirtyTaskTitle();
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new DOMException('quota', 'QuotaExceededError');
    });
    const reload = vi.fn();
    const storage = memoryStorage();
    stop = watchNewVersion({ storage: () => storage, reload });
    expect(preloadError()).toBe(false);
    expect(reload).not.toHaveBeenCalled();
    // 다음 실패 때 다시 시도할 수 있게 표지는 쓰지 않은 채로 둔다.
    expect(storage.getItem(PRELOAD_RELOAD_KEY)).toBeNull();
    const banner = await screen.findByTestId('new-version-banner');
    expect(banner).toHaveTextContent(m.feedback_new_version());
    // 남기지 못했으면 보통 떠나기처럼 경고한다.
    const unload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(true);
  });

  it('reloads once more for a later deploy after the reloaded page stayed up', () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    const reload = vi.fn();
    const storage = memoryStorage();
    storage.setItem(PRELOAD_RELOAD_KEY, '2026-10-27T00:00:00.000Z');
    stop = watchNewVersion({ storage: () => storage, reload });
    vi.advanceTimersByTime(PRELOAD_RELOAD_SETTLE_MS);
    expect(preloadError()).toBe(true);
    expect(reload).toHaveBeenCalledTimes(1);
    expect(newVersion.times).toBe(0);
  });

  it('shows the banner instead of reloading again soon after the reloaded page started, and the banner closes', async () => {
    render(FeedbackHost, { reload: vi.fn() });
    const reload = vi.fn();
    const storage = memoryStorage();
    // 방금 다시 읽은 페이지(표지가 있다). 창 안이라 같은 실패 묶음이다.
    storage.setItem(PRELOAD_RELOAD_KEY, '2026-10-27T00:00:00.000Z');
    stop = watchNewVersion({ storage: () => storage, reload });
    expect(preloadError()).toBe(false);
    expect(reload).not.toHaveBeenCalled();
    const banner = await screen.findByTestId('new-version-banner');
    await fireEvent.click(
      // 배너 안의 닫기 단추(D-82).
      [...banner.querySelectorAll('button')].find((b) => b.textContent === m.common_close()) ??
        banner,
    );
    expect(screen.queryByTestId('new-version-banner')).toBeNull();
    // 닫은 뒤 또 실패하면 다시 보인다.
    preloadError();
    expect(await screen.findByTestId('new-version-banner')).toBeInTheDocument();
  });

  it('turns the draft-keeping mode off and shows the banner when the reload does not happen', () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    const reload = vi.fn();
    const storage = memoryStorage();
    stop = watchNewVersion({ storage: () => storage, reload });
    expect(preloadError()).toBe(true);
    expect(leavingWithDrafts()).toBe(true);
    vi.advanceTimersByTime(PRELOAD_RELOAD_STALL_MS);
    // 창 떠나기 경고가 다시 보통 규칙으로 돌아오고, 저장한 뒤 새로고침하라고 알린다.
    expect(leavingWithDrafts()).toBe(false);
    expect(newVersion.times).toBe(1);
    expect(preloadError()).toBe(false);
  });

  it('reloads with nothing to keep when no form is dirty', () => {
    const reload = vi.fn();
    const storage = memoryStorage();
    stop = watchNewVersion({ storage: () => storage, reload });
    expect(preloadError()).toBe(true);
    expect(reload).toHaveBeenCalledTimes(1);
  });
});
