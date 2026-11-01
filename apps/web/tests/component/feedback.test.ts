import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { toast } from 'svelte-sonner';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
import {
  connection,
  dismissDone,
  notifyDone,
  notifyError,
  auth,
  reportConnectionLost,
} from '$lib/features/feedback/feedback.svelte';

beforeEach(() => {
  // jsdom에는 matchMedia가 없다(좁은 화면 판정에 쓴다).
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
});

afterEach(() => {
  cleanup();
  toast.dismiss();
  connection.lost = false;
  vi.restoreAllMocks();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe('user feedback (D-82)', () => {
  it('shows one toast for repeated completions and offers a close button', async () => {
    render(FeedbackHost);
    notifyDone('저장했습니다.');
    notifyDone('저장했습니다.');
    notifyDone('저장했습니다.');
    await waitFor(() => {
      expect(screen.getAllByText('저장했습니다.')).toHaveLength(1);
    });
    expect(document.querySelector('[data-close-button]')).not.toBeNull();
  });

  function showAction() {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-11-01T00:00:00Z'));
    const onClick = vi.fn();
    render(FeedbackHost);
    flushSync(() => {
      notifyDone('옮겼습니다.', 'timed-action', { action: { label: '되돌리기', onClick } });
    });
    const action = screen.getByRole('button', { name: '되돌리기' });
    const item = action.closest('[data-sonner-toast]');
    if (!(item instanceof HTMLElement)) throw new Error('toast');
    const region = item.closest('[data-sonner-toaster]');
    if (!(region instanceof HTMLElement)) throw new Error('toaster');
    return { onClick, action, item, region };
  }

  async function advance(ms: number) {
    await vi.advanceTimersByTimeAsync(ms);
    flushSync();
  }

  async function expectRemainingSixSeconds(item: HTMLElement) {
    await advance(5_999);
    expect(item).toHaveAttribute('data-removed', 'false');
    await advance(1);
    expect(item).toHaveAttribute('data-removed', 'true');
    // sonner의 퇴장 애니메이션 뒤 DOM에서도 없어진다.
    await advance(200);
    expect(item).not.toBeInTheDocument();
  }

  it('closes an action toast after ten seconds and keeps polite live announcements', async () => {
    const { item } = showAction();
    expect(item).toHaveAttribute('aria-live', 'polite');
    expect(item.closest('section')).toHaveAttribute('aria-live', 'polite');
    await advance(9_999);
    expect(item).toHaveAttribute('data-removed', 'false');
    await advance(1);
    expect(item).toHaveAttribute('data-removed', 'true');
    await advance(200);
    expect(item).not.toBeInTheDocument();
  });

  it('pauses while hovered and resumes the remaining time', async () => {
    const { item, region } = showAction();
    await advance(4_000);
    await fireEvent.mouseEnter(region, { clientX: 20, clientY: 20 });
    await advance(20_000);
    expect(item).toHaveAttribute('data-removed', 'false');
    await fireEvent.mouseLeave(region, { clientX: 100, clientY: 100 });
    await expectRemainingSixSeconds(item);
  });

  it.each(['button', 'Alt+T'])(
    'pauses with %s focus and resumes the remaining time',
    async (via) => {
      const { item, action, region } = showAction();
      await advance(4_000);
      if (via === 'Alt+T') {
        await fireEvent.keyDown(document, { altKey: true, code: 'KeyT', key: 't' });
        expect(region).toHaveFocus();
      } else {
        action.focus();
        flushSync();
        expect(action).toHaveFocus();
      }
      await advance(20_000);
      expect(item).toHaveAttribute('data-removed', 'false');
      // 영역 안의 초점 이동·Escape도 머무는 초점을 풀지 않는다.
      action.focus();
      await fireEvent.keyDown(action, { code: 'Escape', key: 'Escape' });
      await advance(20_000);
      expect(item).toHaveAttribute('data-removed', 'false');
      action.blur();
      flushSync();
      await expectRemainingSixSeconds(item);
    },
  );

  it('pauses while the document is hidden and resumes the remaining time', async () => {
    const visibility = vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible');
    vi.spyOn(document, 'hidden', 'get').mockImplementation(
      () => document.visibilityState === 'hidden',
    );
    const { item } = showAction();
    await advance(4_000);
    visibility.mockReturnValue('hidden');
    await fireEvent(document, new Event('visibilitychange'));
    await advance(20_000);
    expect(item).toHaveAttribute('data-removed', 'false');
    visibility.mockReturnValue('visible');
    await fireEvent(document, new Event('visibilitychange'));
    await expectRemainingSixSeconds(item);
  });

  it('runs undo and supports closing without running it', async () => {
    const { onClick, action } = showAction();
    await fireEvent.click(action);
    expect(onClick).toHaveBeenCalledOnce();
    await advance(200);
    flushSync(() => {
      notifyDone('옮겼습니다.', 'timed-action', { action: { label: '되돌리기', onClick } });
    });
    const close = screen
      .getByRole('button', { name: '되돌리기' })
      .closest('[data-sonner-toast]')
      ?.querySelector('[data-close-button]');
    if (!(close instanceof HTMLElement)) throw new Error('close button');
    await fireEvent.click(close);
    await advance(200);
    expect(screen.queryByRole('button', { name: '되돌리기' })).toBeNull();
    expect(onClick).toHaveBeenCalledOnce();
  });

  it('resumes only after every overlapping pause has ended', async () => {
    const visibility = vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible');
    const { item, action, region } = showAction();
    await advance(4_000);
    await fireEvent.mouseEnter(region);
    action.focus();
    visibility.mockReturnValue('hidden');
    await fireEvent(document, new Event('visibilitychange'));
    await advance(20_000);
    await fireEvent.mouseLeave(region, { clientX: 100, clientY: 100 });
    await advance(20_000);
    expect(item).toHaveAttribute('data-removed', 'false');
    action.blur();
    flushSync();
    await advance(20_000);
    expect(item).toHaveAttribute('data-removed', 'false');
    visibility.mockReturnValue('visible');
    await fireEvent(document, new Event('visibilitychange'));
    await expectRemainingSixSeconds(item);
  });

  it('keeps a replacement action paused and cancels the dismissed action timer', async () => {
    const { region, onClick } = showAction();
    await advance(4_000);
    await fireEvent.mouseEnter(region);
    flushSync(() => {
      notifyDone('다시 옮겼습니다.', 'timed-action', { action: { label: '되돌리기', onClick } });
    });
    const item = screen.getByText('다시 옮겼습니다.').closest('[data-sonner-toast]');
    await advance(20_000);
    expect(item).toHaveAttribute('data-removed', 'false');
    await fireEvent.mouseLeave(region, { clientX: 100, clientY: 100 });
    await advance(9_999);
    expect(item).toHaveAttribute('data-removed', 'false');
    flushSync(() => dismissDone('timed-action'));
    await advance(200);
    expect(item).not.toBeInTheDocument();
    flushSync(() => {
      notifyDone('새 이동입니다.', 'timed-action', { action: { label: '되돌리기', onClick } });
    });
    await advance(9_999);
    const latest = screen.getByText('새 이동입니다.').closest('[data-sonner-toast]');
    expect(latest).toHaveAttribute('data-removed', 'false');
    await advance(1);
    expect(latest).toHaveAttribute('data-removed', 'true');
  });

  it('keeps the library default duration for a completion without an action', async () => {
    showAction();
    flushSync(() => notifyDone('되돌렸습니다.', 'timed-action'));
    const item = screen.getByText('되돌렸습니다.').closest('[data-sonner-toast]');
    await advance(3_999);
    expect(item).toHaveAttribute('data-removed', 'false');
    await advance(1);
    expect(item).toHaveAttribute('data-removed', 'true');
  });

  it('keeps only the latest action for a key and gives it ten seconds', async () => {
    const { onClick } = showAction();
    await advance(4_000);
    const latest = vi.fn();
    flushSync(() => {
      notifyDone('다시 옮겼습니다.', 'timed-action', {
        action: { label: '최근 이동 되돌리기', onClick: latest },
      });
    });
    expect(screen.queryByText('옮겼습니다.')).toBeNull();
    expect(screen.getAllByText('다시 옮겼습니다.')).toHaveLength(1);
    expect(screen.queryByRole('button', { name: '되돌리기' })).toBeNull();
    await advance(9_999);
    await fireEvent.click(screen.getByRole('button', { name: '최근 이동 되돌리기' }));
    expect(latest).toHaveBeenCalledOnce();
    expect(onClick).not.toHaveBeenCalled();
  });

  // 칸반 이동: 되돌리기 단추가 달린 완료를 같은 key의 단추 없는 완료(되돌림)로 고치면 앞 단추가 남지 않는다. 토스트
  // 라이브러리는 같은 id를 고칠 때 앞 값에 합치므로 notifyDone이 앞 토스트를 내리고 새로 띄운다.
  it('drops the earlier action when the same key is updated without one', async () => {
    render(FeedbackHost);
    notifyDone('옮겼습니다.', 'board-move-test', {
      action: { label: '되돌리기', onClick: () => undefined },
    });
    await screen.findByRole('button', { name: '되돌리기' });
    notifyDone('되돌렸습니다.', 'board-move-test');
    await screen.findByText('되돌렸습니다.');
    await waitFor(() => {
      expect(screen.queryByRole('button', { name: '되돌리기' })).toBeNull();
    });
  });

  it('takes a completion toast down by its key', async () => {
    render(FeedbackHost);
    notifyDone('옮겼습니다.', 'dismiss-test', {
      action: { label: '되돌리기', onClick: () => undefined },
    });
    await screen.findByRole('button', { name: '되돌리기' });
    dismissDone('dismiss-test');
    await waitFor(() => {
      expect(screen.queryByText('옮겼습니다.')).toBeNull();
    });
  });

  it('shows a connection banner until the server answers again', async () => {
    vi.useFakeTimers();
    vi.stubGlobal('fetch', () => Promise.resolve(new Response('{}', { status: 200 })));
    render(FeedbackHost);
    reportConnectionLost();
    await vi.waitFor(() => {
      expect(screen.getByTestId('connection-banner').parentElement).toHaveAttribute(
        'role',
        'status',
      );
    });
    await vi.advanceTimersByTimeAsync(5_000);
    await vi.waitFor(() => {
      expect(screen.queryByTestId('connection-banner')).toBeNull();
    });
  });

  it('updates a repeated error and dismisses banners without changing connection or session state', async () => {
    render(FeedbackHost);
    notifyError('연결 오류', 'same-error');
    notifyError('연결 오류 갱신', 'same-error');
    await waitFor(() => {
      expect(screen.getAllByText('연결 오류 갱신')).toHaveLength(1);
    });
    // 같은 key는 첫 토스트를 고쳐 쓴다. 새 토스트가 하나 더 생기면 첫 글이 남는다(R5 B 최종 FB-2).
    expect(screen.queryByText('연결 오류')).toBeNull();
    expect(
      [...document.querySelectorAll('[data-sonner-toast]')].filter((toast) =>
        toast.textContent.includes('연결 오류'),
      ),
    ).toHaveLength(1);
    auth.expired = true;
    connection.lost = true;
    await waitFor(() => {
      expect(screen.getAllByRole('button', { name: '닫기' })).toHaveLength(2);
    });
    for (const button of screen.getAllByRole('button', { name: '닫기' }))
      await fireEvent.click(button);
    expect(screen.queryByTestId('connection-banner')).toBeNull();
    expect(screen.queryByTestId('session-banner')).toBeNull();
    expect(connection.lost).toBe(true);
    expect(auth.expired).toBe(true);
    auth.expired = false;
  });
});

describe('connection detection (D-82)', () => {
  it('treats a proxy gateway error as a lost connection but not an app error', async () => {
    const { api } = await import('$lib/api/client');
    vi.stubGlobal('fetch', () => Promise.resolve(new Response('Bad Gateway', { status: 502 })));
    await expect(api.document('x')).rejects.toThrow();
    expect(connection.lost).toBe(true);
    connection.lost = false;
    vi.stubGlobal('fetch', () =>
      Promise.resolve(
        new Response('{"error":"unavailable","message":"search is down"}', { status: 503 }),
      ),
    );
    await expect(api.document('x')).rejects.toThrow();
    expect(connection.lost).toBe(false);
  });
});

describe('server failures (D-83)', () => {
  it('shows a safe message with the request id, not server details', async () => {
    const { api, ApiError } = await import('$lib/api/client');
    const { setLocale } = await import('$lib/paraglide/runtime.js');
    await setLocale('en', { reload: false });
    vi.stubGlobal('fetch', () =>
      Promise.resolve(
        new Response(
          '{"error":"internal","message":"Internal Server Error","request_id":"0199-test-id"}',
          { status: 500 },
        ),
      ),
    );
    const error: unknown = await api.document('x').catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ApiError);
    if (!(error instanceof ApiError)) return;
    expect(error.requestId).toBe('0199-test-id');
    expect(error.code).toBe('internal');
    expect(error.message).toContain('0199-test-id');
    expect(error.message).not.toContain('Internal Server Error');
    // 입력 오류(4xx)는 서버가 준 설명 그대로.
    vi.stubGlobal('fetch', () =>
      Promise.resolve(
        new Response('{"error":"bad_request","message":"title is empty","request_id":"r-0001"}', {
          status: 400,
        }),
      ),
    );
    await expect(api.document('x')).rejects.toThrow('title is empty');
  });
});

describe('error text by code (D-81)', () => {
  it('translates the reason by error code and keeps English details only in English', async () => {
    const { ApiError, NetworkError } = await import('$lib/api/client');
    const { errorText } = await import('$lib/api/errors');
    const { setLocale } = await import('$lib/paraglide/runtime.js');
    await setLocale('ko', { reload: false });
    expect(errorText(new ApiError(403, 'forbidden', 'forbidden'), '저장하지 못했습니다.')).toBe(
      '저장하지 못했습니다. 권한이 없습니다.',
    );
    expect(
      errorText(new ApiError(400, 'bad_request', 'title is empty'), '저장하지 못했습니다.'),
    ).not.toContain('title is empty');
    expect(errorText(new NetworkError('x'), '저장하지 못했습니다.')).toContain('연결');
    expect(errorText(new Error('boom'), '저장하지 못했습니다.')).toBe('저장하지 못했습니다.');
    await setLocale('en', { reload: false });
    expect(errorText(new ApiError(400, 'bad_request', 'title is empty'), "Couldn't save.")).toBe(
      "Couldn't save. title is empty",
    );
    // 5xx는 client.ts가 만든 안전한 문구(요청 ID 포함) 그대로.
    expect(errorText(new ApiError(500, 'internal', 'safe text r-1', 'r-1'), "Couldn't save.")).toBe(
      'safe text r-1',
    );
  });
});

describe('responses without the app error body (D-82)', () => {
  it('explain an unreachable server in the UI language instead of a raw status', async () => {
    const { api, ApiError } = await import('$lib/api/client');
    const { errorText } = await import('$lib/api/errors');
    const { setLocale } = await import('$lib/paraglide/runtime.js');
    await setLocale('ko', { reload: false });
    vi.stubGlobal('fetch', () => Promise.resolve(new Response('', { status: 502 })));
    const error: unknown = await api.document('x').catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ApiError);
    if (!(error instanceof ApiError)) return;
    expect(error.code).toBe('unreachable');
    expect(error.message).not.toContain('502');
    expect(errorText(error, '저장하지 못했습니다.')).toMatch(
      /^저장하지 못했습니다\. 서버에 닿지 않았습니다/,
    );
    connection.lost = false;
    vi.stubGlobal('fetch', () => Promise.resolve(new Response('<html>', { status: 418 })));
    const odd: unknown = await api.document('x').catch((e: unknown) => e);
    expect(odd instanceof ApiError && odd.message).toBe('예상하지 못한 응답을 받았습니다(418).');
  });
});
