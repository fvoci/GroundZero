import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocumentView, UploadView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import DocumentEditor from '$lib/features/documents/DocumentEditor.svelte';
import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
import { uploadFile } from '$lib/features/files/upload';
import { auth, connection } from '$lib/features/feedback/feedback.svelte';
import { me as currentUser } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// #60·이슈 #88(사용자 결정 2026-10-22): 보내기 전의 `/api/me` 확인만으로는 확인과 전송 사이에 다른 탭에서 계정을
// 바꾸는 경우를 막지 못한다. 저장 요청은 편집을 시작한 사용자 id를 `x-fvoci-expected-user`로 보내고, 서버가
// 세션의 사용자와 다르면 409 `account_changed`로 거절한다. 화면은 다시 보내지 않고, 입력을 앞 계정의 초안
// (사용자별 키)에 남긴 채 계정 전환 처리(다시 읽기)로 넘긴다.
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

/** 보낸 저장 요청의 기대 사용자 머리(없으면 null). */
let saves: (string | null)[] = [];
/** 첫 저장을 연결 오류로 실패시킬지. */
let failFirst = false;

/**
 * 가짜 서버: `/api/me`는 언제나 앞 계정(u1)이라 보내기 전 확인은 통과한다. 그 확인과 저장 전송 사이에 다른 탭이
 * u2로 로그인해 쿠키가 바뀌었으므로, 저장은 서버가 세션(u2)과 기대 사용자를 비교해 409 `account_changed`로 거절한다.
 */
function stubServer(): void {
  vi.stubGlobal('fetch', (input: RequestInfo | URL, init?: RequestInit) => {
    if (input === '/api/me') {
      return Promise.resolve(json(200, { id: 'u1', display_name: '나', email: null, limits }));
    }
    if (input === '/api/documents/doc-1' && init?.method === 'PUT') {
      const headers = new Headers(init.headers);
      saves.push(headers.get('x-fvoci-expected-user'));
      if (failFirst && saves.length === 1) return Promise.reject(new TypeError('Failed to fetch'));
      return Promise.resolve(
        json(409, {
          error: 'account_changed',
          message: 'the signed-in account is not the one this edit was started with',
        }),
      );
    }
    return Promise.resolve(json(404, { error: 'not_found', message: 'Not Found' }));
  });
}

const drafts = (user: string): string[] =>
  Object.keys(window.localStorage).filter((k) => k.startsWith(`fvoci:draft:${user}:doc-1`));

async function editTitle(): Promise<void> {
  const title = document.getElementById('doc-title');
  if (!(title instanceof HTMLInputElement)) throw new Error('doc-title');
  await fireEvent.input(title, { target: { value: '앞 계정의 편집' } });
}

describe('the server refuses a save sent after the account changed', () => {
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
    saves = [];
    failFirst = false;
    stubServer();
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    auth.expired = false;
    connection.lost = false;
    currentUser.userId = '';
    window.localStorage.clear();
  });

  /** 거절 뒤: 다시 보내지 않고, 앞 계정의 초안을 남기고, 계정 전환 처리로 넘겼다. */
  async function expectStoppedAndKept(reload: () => void): Promise<void> {
    const sent = saves.length;
    await vi.advanceTimersByTimeAsync(RETRY_MS * 3);
    expect(saves).toHaveLength(sent);
    expect(saves.every((s) => s === 'u1')).toBe(true);
    expect(reload).toHaveBeenCalledOnce();
    expect(drafts('u1').length).toBeGreaterThan(0);
    expect(drafts('u2')).toEqual([]);
    expect(screen.getByTestId('save-status')).not.toHaveTextContent('재시도 대기');
  }

  it('stops an autosave, keeps the draft for the first account and reloads', async () => {
    const reload = vi.fn();
    render(FeedbackHost, { props: { reload } });
    render(DocumentEditor, { props: { initial: doc } });
    await editTitle();
    await vi.advanceTimersByTimeAsync(AUTOSAVE_MS);
    expect(saves).toEqual(['u1']);
    await expectStoppedAndKept(reload);
    expect(screen.getByRole('alert')).toHaveTextContent(
      '다른 계정으로 로그인돼 저장하지 않았습니다',
    );

    // 다시 읽기 전에 더 고쳐도 보내지 않고 앞 계정의 초안만 고친다.
    const title = document.getElementById('doc-title');
    if (!(title instanceof HTMLInputElement)) throw new Error('doc-title');
    await fireEvent.input(title, { target: { value: '앞 계정의 편집 더' } });
    await vi.advanceTimersByTimeAsync(AUTOSAVE_MS + RETRY_MS);
    expect(saves).toEqual(['u1']);
    const kept = drafts('u1').map((k) => window.localStorage.getItem(k) ?? '');
    expect(kept.some((v) => v.includes('앞 계정의 편집 더'))).toBe(true);
    expect(drafts('u2')).toEqual([]);
  });

  it('stops a direct save the same way', async () => {
    const reload = vi.fn();
    render(FeedbackHost, { props: { reload } });
    render(DocumentEditor, { props: { initial: doc } });
    await editTitle();
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await vi.advanceTimersByTimeAsync(0);
    expect(saves).toEqual(['u1']);
    await expectStoppedAndKept(reload);
  });

  it('stops a waiting retry the same way', async () => {
    const reload = vi.fn();
    failFirst = true;
    render(FeedbackHost, { props: { reload } });
    render(DocumentEditor, { props: { initial: doc } });
    await editTitle();
    await vi.advanceTimersByTimeAsync(AUTOSAVE_MS);
    expect(screen.getByTestId('save-status')).toHaveTextContent('재시도 대기');
    await vi.advanceTimersByTimeAsync(RETRY_MS);
    expect(saves).toEqual(['u1', 'u1']);
    await expectStoppedAndKept(reload);
  });
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
