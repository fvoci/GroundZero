import { fireEvent, render } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { DocumentView } from '$lib/api/client';
import { api } from '$lib/api/client';
import DocumentEditor from '$lib/features/documents/DocumentEditor.svelte';
import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
import { auth } from '$lib/features/feedback/feedback.svelte';
import { me as currentUser } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

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

/** 창을 떠나려 할 때 저장 안 한 편집 경고가 떠나기를 막는지. */
function unloadBlocked(): boolean {
  const event = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(event);
  return event.defaultPrevented;
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  auth.expired = false;
  currentUser.userId = '';
  window.localStorage.clear();
});

// 리뷰 지적: 새 탭에서 다른 계정으로 로그인해 이 창을 다시 읽을 때, 저장 안 한 문서 편집이 있으면 떠나기 경고가
// 다시 읽기를 막을 수 있었다. 취소하면 앞 계정의 화면과 입력이 남은 채 요청은 새 계정으로 나갔다. 입력은 앞 계정의
// 초안(사용자별 키)으로 이미 남아 있으므로, 계정이 바뀐 다시 읽기에서는 경고하지 않는다.
describe('account switch reload and the unsaved-edit warning', () => {
  it('warns on leaving with unsaved edits, but not when reloading for another account', async () => {
    await setLocale('ko', { reload: false });
    currentUser.userId = 'u1';
    vi.stubGlobal('matchMedia', (query: string) => ({
      matches: false,
      media: query,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
    }));
    // 저장은 끝나지 않게 해 편집이 저장 안 한 채로 남게 한다.
    vi.spyOn(api, 'saveDocument').mockReturnValue(new Promise(() => undefined));
    let blockedOnReload: boolean | undefined;
    render(FeedbackHost, {
      props: {
        reload: () => {
          blockedOnReload = unloadBlocked();
        },
      },
    });
    render(DocumentEditor, { props: { initial: doc } });

    const title = document.getElementById('doc-title');
    if (!(title instanceof HTMLInputElement)) throw new Error('doc-title');
    await fireEvent.input(title, { target: { value: '고친 제목' } });
    // 보통 떠나기는 여전히 경고한다.
    expect(unloadBlocked()).toBe(true);

    // 세션이 끝난 뒤 새 탭에서 다른 계정(u2)으로 로그인하고 이 창으로 돌아온다.
    vi.stubGlobal('fetch', () => Promise.resolve(json(401, { error: 'unauthorized' })));
    await expect(api.workspaces()).rejects.toThrow();
    expect(auth.expired).toBe(true);
    vi.stubGlobal('fetch', (input: RequestInfo | URL) =>
      Promise.resolve(
        input === '/api/me'
          ? json(200, { id: 'u2', display_name: '다른 사람', email: null })
          : json(404, { error: 'not_found' }),
      ),
    );
    await fireEvent.focus(window);

    await vi.waitFor(() => {
      expect(blockedOnReload).toBe(false);
    });
    // 입력은 앞 계정의 초안으로 남아 있다(다시 로그인하면 되살릴 수 있다).
    const kept = Object.keys(window.localStorage).filter((k) =>
      k.startsWith('fvoci:draft:u1:doc-1'),
    );
    expect(kept.length).toBeGreaterThan(0);
  });

  // 리뷰 지적(81c9d99): 이 브라우저에 초안을 남기지 못하면(저장소를 쓸 수 없음) 계정이 바뀐 다시 읽기가 경고 없이
  // 입력을 버렸다. 그때는 보통 떠나기처럼 경고한다(남으면 저장은 계정 확인에서 막힌다).
  it('still warns when the edit could not be kept as a draft for the first account', async () => {
    await setLocale('ko', { reload: false });
    currentUser.userId = 'u1';
    vi.stubGlobal('matchMedia', (query: string) => ({
      matches: false,
      media: query,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
    }));
    vi.spyOn(api, 'saveDocument').mockReturnValue(new Promise(() => undefined));
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('QuotaExceededError');
    });
    let blockedOnReload: boolean | undefined;
    render(FeedbackHost, {
      props: {
        reload: () => {
          blockedOnReload = unloadBlocked();
        },
      },
    });
    render(DocumentEditor, { props: { initial: doc } });

    const title = document.getElementById('doc-title');
    if (!(title instanceof HTMLInputElement)) throw new Error('doc-title');
    await fireEvent.input(title, { target: { value: '고친 제목' } });

    vi.stubGlobal('fetch', (input: RequestInfo | URL) =>
      Promise.resolve(
        input === '/api/me'
          ? json(200, { id: 'u2', display_name: '다른 사람', email: null })
          : json(404, { error: 'not_found' }),
      ),
    );
    await fireEvent.focus(window);

    await vi.waitFor(() => {
      expect(blockedOnReload).toBe(true);
    });
  });
});
