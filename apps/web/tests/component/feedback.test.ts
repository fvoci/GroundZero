import { render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
import {
  connection,
  notifyDone,
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

  it('shows a connection banner until the server answers again', async () => {
    vi.useFakeTimers();
    vi.stubGlobal('fetch', () => Promise.resolve(new Response('{}', { status: 200 })));
    render(FeedbackHost);
    reportConnectionLost();
    await vi.waitFor(() => {
      expect(screen.getByTestId('connection-banner').getAttribute('role')).toBe('status');
    });
    await vi.advanceTimersByTimeAsync(5_000);
    await vi.waitFor(() => {
      expect(screen.queryByTestId('connection-banner')).toBeNull();
    });
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
