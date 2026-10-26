import type { Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocumentView } from '$lib/api/client';
import { api } from '$lib/api/client';
import DocumentActions from '$lib/features/documents/DocumentActions.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

const workspaces: Workspace[] = [
  {
    id: 'ws-1',
    organization_id: 'org-1',
    name: '공간',
    role: 'admin',
    is_owner: true,
    realtime_enabled: false,
  },
];

function documentWith(visibility: 'workspace' | 'private', allowCopy: boolean): DocumentView {
  return {
    id: 'd1',
    workspace_id: 'ws-1',
    placement: 'wiki',
    item_id: null,
    parent_id: null,
    reference: 'WIKI-1',
    title: '문서',
    body: { type: 'doc' },
    schema_version: 1,
    revision: 3,
    updated_at: '2026-10-19T00:00:00Z',
    access: 'owner',
    can_edit: true,
    visibility,
    allow_copy: allowCopy,
    can_manage: true,
    can_delete: true,
    is_template: false,
    realtime: false,
    can_export: true,
  };
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.spyOn(api, 'shares').mockResolvedValue([]);
  vi.spyOn(api, 'publicLinks').mockResolvedValue([]);
});

afterEach(() => {
  vi.restoreAllMocks();
});

// 감사 W6: 이 화면이 문서를 연 뒤 다른 소유자가 "비공개"로 바꿨다. 여기서 "복제 허용"만 바꿀 때 화면이 들고 있던 옛
// 공개 범위(워크스페이스)를 함께 보내 비공개를 되돌리지 않는다. 바꾼 칸만 보내고 나머지는 서버가 정한다(0085).
describe('document settings from an out-of-date view', () => {
  it('sends only the setting the user changed', async () => {
    // 화면의 문서는 아직 "워크스페이스 공개"다. 서버에서는 그사이 비공개가 됐다.
    vi.spyOn(api, 'document').mockResolvedValue(documentWith('private', true));
    const set = vi
      .spyOn(api, 'setSettings')
      .mockImplementation((_id, change) =>
        Promise.resolve(documentWith(change.visibility ?? 'private', change.allow_copy ?? true)),
      );
    render(DocumentActions, {
      props: { doc: documentWith('workspace', true), workspaces, onchanged: () => undefined },
    });

    await fireEvent.click(screen.getByRole('button', { name: '공유·설정' }));
    await fireEvent.click(
      await screen.findByRole('checkbox', { name: '볼 수 있는 사람의 복제 허용' }),
    );
    await waitFor(() => {
      expect(set).toHaveBeenCalledTimes(1);
    });
    // 바꾼 칸(복제 허용)만 보낸다. 공개 범위는 서버가 지금 값(비공개) 그대로 둔다.
    expect(set).toHaveBeenCalledWith('d1', { allow_copy: false });
  });

  it('shows the current server settings when the dialog opens', async () => {
    vi.spyOn(api, 'document').mockResolvedValue(documentWith('private', false));
    const changed = vi.fn<(doc: DocumentView) => void>();
    render(DocumentActions, {
      props: { doc: documentWith('workspace', true), workspaces, onchanged: changed },
    });

    await fireEvent.click(screen.getByRole('button', { name: '공유·설정' }));
    await waitFor(() => {
      expect(changed).toHaveBeenCalledWith(
        expect.objectContaining({ id: 'd1', visibility: 'private', allow_copy: false }),
      );
    });
  });

  it('does not send a second change built on the first one’s stale value', async () => {
    vi.spyOn(api, 'document').mockResolvedValue(documentWith('workspace', true));
    let finish: () => void = () => undefined;
    const set = vi.spyOn(api, 'setSettings').mockImplementation(
      (_id, change) =>
        new Promise((resolve) => {
          finish = () => {
            resolve(documentWith(change.visibility ?? 'workspace', change.allow_copy ?? true));
          };
        }),
    );
    render(DocumentActions, {
      props: { doc: documentWith('workspace', true), workspaces, onchanged: () => undefined },
    });
    await fireEvent.click(screen.getByRole('button', { name: '공유·설정' }));

    // 비공개를 누르고, 응답이 오기 전에 복제 허용도 누른다.
    await fireEvent.click(await screen.findByRole('checkbox', { name: /^비공개/ }));
    await waitFor(() => {
      expect(set).toHaveBeenCalledTimes(1);
    });
    expect(set).toHaveBeenLastCalledWith('d1', { visibility: 'private' });
    await fireEvent.click(screen.getByRole('checkbox', { name: '볼 수 있는 사람의 복제 허용' }));
    await new Promise((r) => setTimeout(r, 0));
    // 첫 요청이 끝나기 전에는 두 번째 요청을 보내지 않는다(응답 전 화면은 첫 변경 전의 값을 보인다).
    expect(set).toHaveBeenCalledTimes(1);
    finish();
  });

  it('does not change the template setting while another setting is being saved', async () => {
    vi.spyOn(api, 'document').mockResolvedValue(documentWith('workspace', true));
    // 서버의 템플릿 값. 설정 변경은 먼저 처리되어 그때의 값(꺼짐)을 돌려주고, 응답만 늦게 온다.
    let serverTemplate = false;
    let finish: () => void = () => undefined;
    vi.spyOn(api, 'setSettings').mockImplementation((_id, change) => {
      const done: DocumentView = {
        ...documentWith(change.visibility ?? 'workspace', true),
        is_template: serverTemplate,
      };
      return new Promise((resolve) => {
        finish = () => {
          resolve(done);
        };
      });
    });
    const template = vi.spyOn(api, 'setDocumentTemplate').mockImplementation((_id, on) => {
      serverTemplate = on;
      return Promise.resolve(undefined);
    });
    const changed = vi.fn<(doc: DocumentView) => void>();
    render(DocumentActions, {
      props: { doc: documentWith('workspace', true), workspaces, onchanged: changed },
    });
    await fireEvent.click(screen.getByRole('button', { name: '공유·설정' }));

    // 비공개를 누르고, 응답이 오기 전에 템플릿도 누른다.
    await fireEvent.click(await screen.findByRole('checkbox', { name: /^비공개/ }));
    await fireEvent.click(screen.getByRole('checkbox', { name: /^템플릿으로 쓰기/ }));
    await new Promise((r) => setTimeout(r, 0));
    finish();
    await waitFor(() => {
      expect(changed).toHaveBeenLastCalledWith(expect.objectContaining({ visibility: 'private' }));
    });

    // 늦게 온 설정 응답이 템플릿 표시를 서버와 다르게 되돌리지 않는다. 첫 요청이 끝나기 전에는 템플릿을 바꾸지 않는다.
    expect(changed.mock.lastCall?.[0].is_template).toBe(serverTemplate);
    expect(template).not.toHaveBeenCalled();
  });
});

// 리뷰: 설정 저장이 끝나기 전에 창을 닫았다 다시 열면 다시 읽기가 먼저 반영되고, 늦게 온 저장 응답이 그 뒤에 화면을
// 덮었다. 그사이 다른 소유자가 바꾼 값(복제 금지)이 저장 응답의 옛 값으로 되돌아 보였다. 다시 읽은 뒤에 끝난 저장은 한
// 번 더 읽어 서버의 지금 값으로 맞춘다.
describe('a settings save that finishes after the dialog was reopened', () => {
  it('reads the settings again instead of applying the late answer', async () => {
    const reads = [
      documentWith('workspace', true),
      // 다시 열 때: 저장은 아직이고, 다른 소유자가 복제를 막았다.
      documentWith('workspace', false),
      // 저장이 끝난 뒤의 서버 값.
      documentWith('private', false),
    ];
    const read = vi
      .spyOn(api, 'document')
      .mockImplementation(() => Promise.resolve(reads.shift() ?? documentWith('private', false)));
    let finish: () => void = () => undefined;
    vi.spyOn(api, 'setSettings').mockImplementation(
      () =>
        new Promise((resolve) => {
          // 저장 응답은 저장한 순간의 값(복제 허용)이다.
          finish = () => {
            resolve(documentWith('private', true));
          };
        }),
    );
    const changed = vi.fn<(doc: DocumentView) => void>();
    render(DocumentActions, {
      props: { doc: documentWith('workspace', true), workspaces, onchanged: changed },
    });

    await fireEvent.click(screen.getByRole('button', { name: '공유·설정' }));
    await fireEvent.click(await screen.findByRole('checkbox', { name: /^비공개/ }));
    // 저장 중에 창을 닫았다 다시 연다.
    await fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' });
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).toBeNull();
    });
    await fireEvent.click(screen.getByRole('button', { name: '공유·설정' }));
    await waitFor(() => {
      expect(read).toHaveBeenCalledTimes(2);
    });

    finish();
    await waitFor(() => {
      expect(read).toHaveBeenCalledTimes(3);
    });
    await waitFor(() => {
      expect(changed).toHaveBeenLastCalledWith(
        expect.objectContaining({ visibility: 'private', allow_copy: false }),
      );
    });
  });

  // 리뷰(#57 요약): 문서 화면은 다른 문서로 옮기면 이 창을 내리고 돌아오면 새로 만든다(A → B → A). 내린 창이 연 설정
  // 다시 읽기가 늦게 오면 같은 id라 새 화면의 설정 표시를 옛 값으로 덮었다. 내린 창의 결과는 넘기지 않는다.
  it('drops a settings answer that arrives after the dialog left the screen', async () => {
    let answer: (doc: DocumentView) => void = () => undefined;
    const read = vi.spyOn(api, 'document').mockImplementation(
      () =>
        new Promise<DocumentView>((resolve) => {
          answer = resolve;
        }),
    );
    const changed = vi.fn<(doc: DocumentView) => void>();
    const view = render(DocumentActions, {
      props: { doc: documentWith('workspace', true), workspaces, onchanged: changed },
    });
    await fireEvent.click(screen.getByRole('button', { name: '공유·설정' }));
    await waitFor(() => {
      expect(read).toHaveBeenCalledTimes(1);
    });

    view.unmount();
    answer(documentWith('private', false));
    await new Promise((r) => setTimeout(r, 0));
    expect(changed).not.toHaveBeenCalled();
  });

  // 리뷰: 창을 열 때 시작한 다시 읽기가 끝나기 전에 템플릿을 바꾸면 그 다시 읽기가 버려지고, 템플릿 응답에는 다른
  // 설정이 없어 그사이 다른 소유자가 바꾼 비공개·복제 금지가 옛 값으로 남았다. 템플릿 저장 뒤 한 번 더 읽는다.
  it('reads the settings again after a template change that dropped the opening read', async () => {
    const answers: ((doc: DocumentView) => void)[] = [];
    const read = vi.spyOn(api, 'document').mockImplementation(
      () =>
        new Promise<DocumentView>((resolve) => {
          answers.push(resolve);
        }),
    );
    vi.spyOn(api, 'setDocumentTemplate').mockResolvedValue(undefined);
    const changed = vi.fn<(doc: DocumentView) => void>();
    render(DocumentActions, {
      props: { doc: documentWith('workspace', true), workspaces, onchanged: changed },
    });
    await fireEvent.click(screen.getByRole('button', { name: '공유·설정' }));
    await waitFor(() => {
      expect(read).toHaveBeenCalledTimes(1);
    });
    // 여는 다시 읽기가 오기 전에 템플릿을 켠다. 그 사이 서버에서는 비공개·복제 금지가 됐다.
    await fireEvent.click(await screen.findByRole('checkbox', { name: /^템플릿으로 쓰기/ }));
    answers[0]?.(documentWith('private', false));
    await waitFor(() => {
      expect(read).toHaveBeenCalledTimes(2);
    });
    answers[1]?.({ ...documentWith('private', false), is_template: true });
    await waitFor(() => {
      expect(changed).toHaveBeenLastCalledWith(
        expect.objectContaining({ visibility: 'private', allow_copy: false, is_template: true }),
      );
    });
  });
});
