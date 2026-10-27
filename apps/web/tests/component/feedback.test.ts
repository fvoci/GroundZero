import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
import {
  connection,
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
  connection.lost = false;
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

  // FC-1: 복제의 "새 항목 열기"처럼 완료 토스트에 동작 단추 하나를 붙인다. 닫을 때까지 남고 닫기 단추로 닫는다.
  it('adds one action button to a completion toast and keeps it until closed', async () => {
    const onClick = vi.fn();
    render(FeedbackHost);
    notifyDone('복제했습니다.', 'copy', { action: { label: '새 항목 열기', onClick } });
    const action = await screen.findByRole('button', { name: '새 항목 열기' });
    expect(document.querySelector('[data-close-button]')).not.toBeNull();
    await fireEvent.click(action);
    expect(onClick).toHaveBeenCalledOnce();
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
