import type { Workspace } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api, type ItemView } from '$lib/api/client';
import { activityApi, type CommentView } from '$lib/features/activity/api';
import ChecklistForm from '$lib/features/checklists/ChecklistForm.svelte';
import CommentsPanel from '$lib/features/comments/CommentsPanel.svelte';
import { accountSafety } from '$lib/features/feedback/feedback.svelte';
import ModuleScreen from '$lib/features/modules/ModuleScreen.svelte';
import ModulesSection from '$lib/features/modules/ModulesSection.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';
import { unloadBlocked } from './fixtures/leave-warning';
import { permissionState } from './fixtures/permission-state.svelte';

// CR-311-1 (#289): 권한이 빠진 때가 계정 전환·복구 중이면 숨은 입력은 그대로 둔다(D-96). 같은 사용자로 전환이 끝나면
// 그때 숨은 입력을 정리해야 한다. 긴 입력은 되살리기 제안으로 옮기고, 화면 입력은 비워 떠날 때 경고하지 않는다(D-27).
// 정리하는 효과가 계정 안전 상태를 읽지 않으면 전환이 끝나도 다시 돌지 않아 숨은 입력이 경고로 남는다. 강좌
// 패널은 전환 중에 패널을 숨기고 읽지 않으며 끝나면 다시 읽으므로(권한이 전환 중에 바뀌지 않는다) 여기서 보지 않는다.

type Lock = 'switching' | 'restoring';

function lock(kind: Lock): void {
  accountSafety[kind] = true;
}

/** The same user is confirmed: the account-safety lock ends without a reload. */
async function unlock(kind: Lock): Promise<void> {
  accountSafety[kind] = false;
  await tick();
}

const workspace: Workspace = {
  id: 'ws-account-switch',
  organization_id: 'org-1',
  name: '전환 공간',
  access: 'owner',
  role: 'admin',
  is_owner: true,
};

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  localStorage.clear();
  me.userId = 'switch-user';
  vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
});

afterEach(() => {
  cleanup();
  accountSafety.switching = false;
  accountSafety.restoring = false;
  router.guard = undefined;
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  me.userId = '';
});

const target = { kind: 'document' as const, id: 'doc-account-switch' };

it.each(['switching', 'restoring'] as const)(
  'moves a new comment hidden during account %s into an offer once the same user is confirmed',
  async (kind) => {
    vi.spyOn(activityApi, 'comments').mockResolvedValue([]);
    const unsaved = vi.fn();
    const view = render(CommentsPanel, { props: { target, canWrite: true, onunsaved: unsaved } });
    await fireEvent.input(screen.getByLabelText(/새 댓글/), {
      target: { value: '전환 중 숨은 댓글' },
    });
    lock(kind);
    await view.rerender({ target, canWrite: false, onunsaved: unsaved });
    expect(screen.queryByLabelText(/새 댓글/)).toBeNull();
    // 전환 중에는 권한 상실로 다루지 않고 입력을 그대로 둔다(D-96).
    expect(unsaved).toHaveBeenLastCalledWith(true);
    await unlock(kind);
    await waitFor(() => {
      expect(unsaved).toHaveBeenLastCalledWith(false);
    });
    expect(unloadBlocked()).toBe(false);
    const offer = await screen.findByTestId('comment-draft-offer');
    expect(offer).toHaveTextContent('전환 중 숨은 댓글');
    expect(offer).toHaveTextContent(m.common_draft_read_only());
  },
);

it('moves a comment edit hidden during an account switch into an offer once the same user is confirmed', async () => {
  const comment = (): CommentView => ({
    id: 'c1',
    parent_id: null,
    body: '원댓글',
    author_id: 'switch-user',
    author_name: '작성자',
    created_at: '2026-10-26T00:00:00Z',
    edited: false,
    deleted: false,
    resolved: false,
    can_edit: true,
    can_delete: false,
    can_resolve: true,
  });
  vi.spyOn(activityApi, 'comments').mockResolvedValue([comment()]);
  const edit = vi.spyOn(activityApi, 'editComment');
  const unsaved = vi.fn();
  const view = render(CommentsPanel, { props: { target, canWrite: true, onunsaved: unsaved } });
  const root = (await screen.findByText('원댓글')).closest('li');
  if (root === null) throw new Error('comment root');
  await fireEvent.click(within(root).getByRole('button', { name: m.comments_edit() }));
  await fireEvent.input(screen.getByLabelText(m.comments_edit_label()), {
    target: { value: '전환 중 숨은 수정' },
  });
  lock('switching');
  // 전환 중에 부모가 다시 읽은 권한으로 더는 쓸 수 없다(패널은 전환 중 자기 동작과 다시 읽기를 멈춘다).
  await view.rerender({ target, canWrite: false, onunsaved: unsaved });
  await waitFor(() => {
    expect(screen.queryByLabelText(m.comments_edit_label())).toBeNull();
  });
  expect(unsaved).toHaveBeenLastCalledWith(true);
  await unlock('switching');
  await waitFor(() => {
    expect(unsaved).toHaveBeenLastCalledWith(false);
  });
  expect(unloadBlocked()).toBe(false);
  expect(await screen.findByTestId('comment-edit-draft-offer')).toHaveTextContent(
    '전환 중 숨은 수정',
  );
  expect(edit).not.toHaveBeenCalled();
});

it('drops a module template title hidden during an account switch once the same user is confirmed', async () => {
  const template: ItemView = {
    id: 'chosen',
    module_id: 'DEV',
    workspace_id: workspace.id,
    reference: 'DEV-1',
    title: '템플릿',
    description: '',
    visibility: 'workspace',
    access: 'owner',
    archived: false,
    updated_at: '',
    status: 'active',
    is_template: true,
    version: 1,
    starts_on: null,
    ends_on: null,
  };
  vi.spyOn(api, 'module').mockResolvedValue({
    id: 'DEV',
    workspace_id: workspace.id,
    kind: 'project',
    name: '모듈 DEV',
    key: 'DEV',
    previous_keys: [],
    archived: false,
    icon: '',
    description: '',
    access: 'owner',
  });
  vi.spyOn(api, 'items').mockResolvedValue([template]);
  const copy = vi.spyOn(api, 'copyItem');
  const current = permissionState<Workspace>({ ...workspace, access: 'write' });
  render(ModuleScreen, { workspace: current, moduleId: 'DEV' });
  await screen.findByRole('heading', { name: '모듈 DEV' });
  await fireEvent.input(screen.getByLabelText(m.modules_template_item_title()), {
    target: { value: '전환 중 숨은 제목' },
  });
  lock('switching');
  current.access = 'read';
  await waitFor(() => {
    expect(screen.queryByLabelText(m.modules_template_item_title())).toBeNull();
  });
  await unlock('switching');
  await waitFor(() => {
    expect(unloadBlocked()).toBe(false);
  });
  expect(router.guard?.(vi.fn(), '/')).toBe(true);
  current.access = 'write';
  expect(await screen.findByLabelText(m.modules_template_item_title())).toHaveValue('');
  expect(copy).not.toHaveBeenCalled();
});

it('drops a new module input hidden during an account switch once the same user is confirmed', async () => {
  const create = vi.spyOn(api, 'createModule');
  const current = permissionState({ ...workspace });
  render(ModulesSection, { workspace: current });
  await fireEvent.input(screen.getByLabelText(m.modules_name()), {
    target: { value: '전환 중 숨은 모듈' },
  });
  await fireEvent.input(screen.getByLabelText(m.modules_key()), { target: { value: 'SWT' } });
  lock('switching');
  current.access = 'write';
  await waitFor(() => {
    expect(screen.queryByRole('form', { name: m.modules_create() })).toBeNull();
  });
  await unlock('switching');
  await waitFor(() => {
    expect(unloadBlocked()).toBe(false);
  });
  expect(router.guard?.(vi.fn(), '/')).toBe(true);
  current.access = 'owner';
  expect(await screen.findByLabelText(m.modules_name())).toHaveValue('');
  expect(screen.getByLabelText(m.modules_key())).toHaveValue('');
  expect(create).not.toHaveBeenCalled();
});

it('moves a kept checklist draft hidden during an account switch into an offer once the same user is confirmed', async () => {
  const unsaved = vi.fn();
  const first = {
    idPrefix: 'account-switch',
    items: [{ id: 'a', text: '초안 작성', done: false }],
    canEdit: true,
    preserveDraft: true,
    onunsaved: unsaved,
    onadd: vi.fn(() => Promise.resolve()),
    ontoggle: vi.fn(() => Promise.resolve()),
    onrename: vi.fn(() => Promise.resolve()),
    onremove: vi.fn(() => Promise.resolve()),
  };
  const view = render(ChecklistForm, first);
  await fireEvent.input(screen.getByLabelText(m.checklists_new_item()), {
    target: { value: '전환 중 숨은 항목' },
  });
  lock('switching');
  await view.rerender({ ...first, canEdit: false });
  expect(unsaved).toHaveBeenLastCalledWith(true);
  await unlock('switching');
  await waitFor(() => {
    expect(unsaved).toHaveBeenLastCalledWith(false);
  });
  expect(unloadBlocked()).toBe(false);
  expect(await screen.findByTestId('checklist-new-item-draft-offer')).toHaveTextContent(
    '전환 중 숨은 항목',
  );
});
