import { fireEvent, render, screen } from '@testing-library/svelte';
import type { MockInstance } from 'vitest';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocumentView } from '$lib/api/client';
import { api, NetworkError } from '$lib/api/client';
import DocumentEditor from '$lib/features/documents/DocumentEditor.svelte';
import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
import { auth, connection } from '$lib/features/feedback/feedback.svelte';
import { me as currentUser } from '$lib/me.svelte';
import { scopedKey } from '$lib/storage';
import { accountSafety } from '$lib/features/feedback/feedback.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 리뷰 지적(#60 요약): 저장이 연결 오류로 재시도를 기다리는 동안 다른 탭에서 다른 계정으로 로그인하면, 이 창은
// 다시 돌아오기(focus) 전까지 앞 계정의 화면 그대로이고 배경 재시도가 새 계정의 쿠키로 나가 앞 계정의 편집을 새
// 계정의 이름으로 저장했다. 이제 보내기 전에 누구로 로그인돼 있는지 확인한다(확인하지 못하면 보내지 않는다).
function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

// `/api/me` 계약의 서버 입력 한도(#79): 필수 칸이라 계정 응답 mock에도 넣는다(서버 기본값).
const limits = {
  max_title_chars: 200,
  max_document_bytes: 2_000_000,
  max_references_per_document: 500,
  max_document_depth: 10,
};

const doc: DocumentView = {
  id: 'doc-1',
  workspace_id: 'ws-1',
  placement: 'wiki',
  item_id: null,
  parent_id: null,
  reference: null,
  title: '운영 안내',
  body: { type: 'doc', content: [] },
  schema_version: 1,
  revision: 1,
  updated_at: '2026-10-21T00:00:00Z',
  access: 'write',
  can_edit: true,
  visibility: 'workspace',
  allow_copy: true,
  can_manage: false,
  can_delete: false,
  is_template: false,
  realtime: false,
  can_export: true,
};

const AUTOSAVE_MS = 1500;
const RETRY_MS = 5000;

/** `/api/me`가 돌려줄 계정 id. `offline`이면 연결 실패. */
let signedIn = 'u1';

function stubServer(): void {
  vi.stubGlobal('fetch', (input: RequestInfo | URL) => {
    if (input !== '/api/me') return Promise.resolve(json(404, { error: 'not_found' }));
    if (signedIn === 'offline') return Promise.reject(new TypeError('Failed to fetch'));
    return Promise.resolve(json(200, { id: signedIn, display_name: '나', email: null, limits }));
  });
}

/** 제목을 고쳐 자동 저장을 걸고, 첫 저장이 연결 오류로 재시도를 기다리게 한다. */
async function editAndFailOnce(save: MockInstance<typeof api.saveDocument>): Promise<void> {
  save.mockRejectedValueOnce(new NetworkError('offline'));
  const title = document.getElementById('doc-title');
  if (!(title instanceof HTMLInputElement)) throw new Error('doc-title');
  await fireEvent.input(title, { target: { value: '고친 제목' } });
  await vi.advanceTimersByTimeAsync(AUTOSAVE_MS);
  expect(save).toHaveBeenCalledTimes(1);
  expect(screen.getByTestId('save-status')).toHaveTextContent('재시도 대기');
}

const drafts = (user: string): string[] =>
  Object.keys(window.localStorage).filter((k) =>
    k.startsWith(scopedKey('draft:doc-1', user, undefined)),
  );

describe('a background save checks who is signed in before it goes out', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    vi.stubGlobal('matchMedia', (query: string) => ({
      matches: false,
      media: query,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
    }));
    currentUser.userId = 'u1';
    signedIn = 'u1';
    stubServer();
  });
  afterEach(() => {
    accountSafety.switching = false;
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    auth.expired = false;
    connection.lost = false;
    currentUser.userId = '';
    window.localStorage.clear();
  });

  it('does not send a waiting retry once another account signed in, and reloads for that account', async () => {
    const reload = vi.fn();
    const save = vi.spyOn(api, 'saveDocument');
    render(FeedbackHost, { props: { reload } });
    render(DocumentEditor, { props: { initial: doc } });
    await editAndFailOnce(save);

    // 다른 탭에서 u2로 로그인했다. 이 창에는 아직 돌아오지 않았다(focus 없음).
    signedIn = 'u2';
    await vi.advanceTimersByTimeAsync(RETRY_MS);

    expect(save).toHaveBeenCalledTimes(1);
    expect(reload).toHaveBeenCalledOnce();
    // 앞 계정의 편집은 앞 계정의 초안(사용자별 키)으로 남는다.
    expect(drafts('u1').length).toBeGreaterThan(0);
    expect(drafts('u2')).toEqual([]);
  });

  it('keeps the retry waiting without sending when it cannot check who is signed in', async () => {
    const reload = vi.fn();
    const save = vi.spyOn(api, 'saveDocument');
    render(FeedbackHost, { props: { reload } });
    render(DocumentEditor, { props: { initial: doc } });
    await editAndFailOnce(save);

    signedIn = 'offline';
    await vi.advanceTimersByTimeAsync(RETRY_MS);
    expect(save).toHaveBeenCalledTimes(1);
    expect(reload).not.toHaveBeenCalled();
    expect(screen.getByTestId('save-status')).toHaveTextContent('재시도 대기');

    // 다시 닿으면(같은 계정) 기다리던 재시도가 저장을 보낸다.
    signedIn = 'u1';
    save.mockResolvedValueOnce({
      kind: 'saved',
      document: { ...doc, title: '고친 제목', revision: 2 },
    });
    await vi.advanceTimersByTimeAsync(RETRY_MS);
    expect(save).toHaveBeenCalledTimes(2);
    expect(reload).not.toHaveBeenCalled();
  });

  it('sends the retry as before when the same account is still signed in', async () => {
    const reload = vi.fn();
    const save = vi.spyOn(api, 'saveDocument');
    render(FeedbackHost, { props: { reload } });
    render(DocumentEditor, { props: { initial: doc } });
    await editAndFailOnce(save);

    save.mockResolvedValueOnce({
      kind: 'saved',
      document: { ...doc, title: '고친 제목', revision: 2 },
    });
    await vi.advanceTimersByTimeAsync(RETRY_MS);
    expect(save).toHaveBeenCalledTimes(2);
    expect(save.mock.calls[1]?.[1]).toMatchObject({ title: '고친 제목', baseRevision: 1 });
    expect(reload).not.toHaveBeenCalled();
    expect(drafts('u1')).toEqual([]);
  });
});
