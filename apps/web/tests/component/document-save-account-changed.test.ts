import { render } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { UploadView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
import { uploadFile } from '$lib/features/files/upload';
import { accountSafety } from '$lib/features/feedback/feedback.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}
beforeEach(async () => {
  await setLocale('ko', { reload: false });
});
afterEach(() => {
  accountSafety.switching = false;
});

describe('requests that carry the expected user', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('sends it on note creation and turns account_changed into an error plus the switch', async () => {
    const seen: (string | null)[] = [];
    vi.stubGlobal('fetch', (_input: RequestInfo | URL, init?: RequestInit) => {
      seen.push(new Headers(init?.headers).get('x-fvoci-expected-user'));
      return Promise.resolve(json(409, { error: 'account_changed', message: 'changed' }));
    });
    vi.stubGlobal('matchMedia', (query: string) => ({
      matches: false,
      media: query,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
    }));
    const reload = vi.fn();
    render(FeedbackHost, { props: { reload } });
    const failed: unknown = await api.createNote('u1', 'r1', '사본').catch((e: unknown) => e);
    expect(seen).toEqual(['u1']);
    expect(failed instanceof ApiError && failed.code).toBe('account_changed');
    expect(reload).toHaveBeenCalledOnce();
  });
});

describe('an upload restarted on its own', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    window.localStorage.clear();
  });

  // CodeRabbit(#60): 만료된 업로드는 `uploadFile`이 스스로 다시 시작한다. 그 요청도 올리기를 시작한 사용자를 그대로
  // 보내 서버가 비교한다(다른 탭에서 바꾼 계정으로 새 업로드를 만들지 않게).
  it('sends the same expected user when it starts an expired upload again', async () => {
    const expired: UploadView = {
      id: 'up-old',
      state: 'uploading',
      name: 'a.txt',
      size: 3,
      chunk_bytes: 1024,
      parts_total: 1,
      received: [],
      expires_at: '2000-01-01T00:00:00Z',
      file_id: null,
      version: null,
      failure: null,
    };
    const create = vi
      .spyOn(api, 'createUpload')
      .mockResolvedValueOnce(expired)
      .mockRejectedValueOnce(new ApiError(409, 'account_changed', 'changed'));
    const failed: unknown = await uploadFile(new File(['abc'], 'a.txt'), {
      expectedUser: 'u1',
      fileId: 'f1',
    }).catch((e: unknown) => e);
    expect(create.mock.calls.map(([input]) => input.expectedUser)).toEqual(['u1', 'u1']);
    expect(failed instanceof ApiError && failed.code).toBe('account_changed');
  });
});
