import type { Me } from '@fvoci/api-client';
import { cleanup, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import { accountSafety, auth, hideSession } from '$lib/features/feedback/feedback.svelte';
import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
import { me as currentUser } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// #88 감지 시점: 창 focus와 문서가 다시 보이는 visibilitychange 둘 다에서 계정을 확인한다. 두 이벤트가 잇달아 와도 확인
// 요청은 한 번에 하나, 전환·다시 읽기는 한 번이다. 숨겨지는 visibilitychange와 내려간 화면은 확인하지 않는다.
// 합성 이벤트로 핸들러만 검증하는 시험이다. 실제 탭 전환·중단된 다시 읽기는 e2e account-switch.spec.ts가 본다.

const other: Me = {
  id: 'u2',
  display_name: '다른 사람',
  hour_cycle: '',
  locale: 'ko',
  time_zone: '',
  week_start: '',
  theme: 'system',
  limits: {
    max_title_chars: 200,
    max_document_bytes: 2_000_000,
    max_references_per_document: 500,
    max_document_depth: 10,
  },
};

function visibility(state: 'visible' | 'hidden'): void {
  Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => state });
}

describe('account change detection events', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    vi.stubGlobal('matchMedia', (query: string) => ({
      matches: false,
      media: query,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
    }));
    currentUser.userId = 'u1';
    accountSafety.switching = false;
    visibility('visible');
  });
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    currentUser.userId = '';
    accountSafety.switching = false;
    auth.expired = false;
    localStorage.removeItem('fvoci:auth-session');
    visibility('visible');
  });

  it.each(['anonymous', 'other-user'])(
    'uses its own %s response when a newer same-user marker rejects publication',
    async (kind) => {
      const key = 'fvoci:auth-session';
      localStorage.setItem(key, JSON.stringify({ state: 'settled', userId: 'u1', generation: 10 }));
      let finish!: (value: Me | undefined) => void;
      const read = vi.spyOn(api, 'me').mockReturnValue(
        new Promise<Me | undefined>((resolve) => {
          finish = resolve;
        }),
      );
      const reload = vi.fn<() => void>();
      render(FeedbackHost, { reload, syncSessionSignal: () => false });
      window.dispatchEvent(new Event('focus'));
      const marker = JSON.stringify({ state: 'settled', userId: 'u1', generation: 11 });
      localStorage.setItem(key, marker);
      finish(kind === 'anonymous' ? undefined : other);
      if (kind === 'anonymous') await screen.findByRole('link', { name: '새 탭에서 로그인' });
      else
        await waitFor(() => {
          expect(reload).toHaveBeenCalledOnce();
        });
      expect(accountSafety.switching).toBe(true);
      expect(read).toHaveBeenCalledOnce();
      expect(localStorage.getItem(key)).toBe(marker);
    },
  );

  it('rechecks in the new safety generation after the real request discards an older response', async () => {
    let finish!: (response: Response) => void;
    const fetch = vi
      .spyOn(globalThis, 'fetch')
      .mockReturnValueOnce(
        new Promise<Response>((resolve) => {
          finish = resolve;
        }),
      )
      .mockResolvedValue(new Response(JSON.stringify(other)));
    const read = vi.spyOn(api, 'me');
    const reload = vi.fn<() => void>();
    render(FeedbackHost, { reload });
    window.dispatchEvent(new Event('focus'));
    hideSession('reauth');
    finish(new Response(JSON.stringify({ ...other, id: 'u1' })));
    await waitFor(() => {
      expect(reload).toHaveBeenCalledOnce();
    });
    expect(read).toHaveBeenCalledTimes(2);
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it('stops automatic safety-generation rechecks after three requests and stays hidden', async () => {
    const read = vi.spyOn(api, 'me').mockImplementation(() => {
      accountSafety.generation += 1;
      return Promise.resolve({ ...other, id: 'u1' });
    });
    render(FeedbackHost, { reload: vi.fn() });
    window.dispatchEvent(new Event('focus'));
    await screen.findByRole('alert');
    expect(read).toHaveBeenCalledTimes(3);
    expect(accountSafety.switching).toBe(true);
    expect(accountSafety.error).not.toBe('');
  });

  it('checks once for focus and visibilitychange arriving together, then switches and reloads once', async () => {
    let finish: (value: Me) => void = () => undefined;
    const check = vi.spyOn(api, 'me').mockReturnValue(
      new Promise<Me>((resolve) => {
        finish = resolve;
      }),
    );
    const reload = vi.fn<() => void>();
    render(FeedbackHost, { props: { reload } });

    document.dispatchEvent(new Event('visibilitychange'));
    window.dispatchEvent(new Event('focus'));
    document.dispatchEvent(new Event('visibilitychange'));
    expect(check).toHaveBeenCalledTimes(1);

    finish(other);
    await waitFor(() => {
      expect(accountSafety.switching).toBe(true);
    });
    expect(reload).toHaveBeenCalledTimes(1);
    // 전환 중에는 더 확인하지 않는다.
    window.dispatchEvent(new Event('focus'));
    document.dispatchEvent(new Event('visibilitychange'));
    expect(check).toHaveBeenCalledTimes(1);
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('does not check when the document becomes hidden, and stops checking after unmount', async () => {
    const check = vi.spyOn(api, 'me').mockResolvedValue(other);
    const reload = vi.fn<() => void>();
    const view = render(FeedbackHost, { props: { reload } });

    visibility('hidden');
    document.dispatchEvent(new Event('visibilitychange'));
    expect(check).not.toHaveBeenCalled();

    view.unmount();
    visibility('visible');
    document.dispatchEvent(new Event('visibilitychange'));
    window.dispatchEvent(new Event('focus'));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(check).not.toHaveBeenCalled();
    expect(reload).not.toHaveBeenCalled();
  });

  it('leaves the screen alone when the same account is still signed in', async () => {
    const check = vi.spyOn(api, 'me').mockResolvedValue({ ...other, id: 'u1' });
    const reload = vi.fn<() => void>();
    render(FeedbackHost, { props: { reload } });
    window.dispatchEvent(new Event('focus'));
    await waitFor(() => {
      expect(check).toHaveBeenCalledTimes(1);
    });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(accountSafety.switching).toBe(false);
    expect(reload).not.toHaveBeenCalled();
  });
});
