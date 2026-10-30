import './fixtures/task-statuses';
import type { Module, Workspace } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api, type ItemProfileView, type ItemView } from '$lib/api/client';
import { taskData as work } from '$lib/features/tasks/detail-data';
import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
import { me } from '$lib/me.svelte';
import { org } from '$lib/org.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { writeSession } from '$lib/storage';
import { resetRoute } from './fixtures/item-groups';

const workspace: Workspace = {
  id: 'ws-primary',
  organization_id: 'org-1',
  name: '공간',
  access: 'owner',
  role: 'admin',
  is_owner: true,
};
const mod: Module = {
  id: 'mod-primary',
  workspace_id: workspace.id,
  kind: 'project',
  name: '프로젝트',
  key: 'PRJ',
  previous_keys: [],
  archived: false,
  icon: '',
  description: '',
  access: 'owner',
};
const item: ItemView = {
  id: 'item-primary',
  module_id: mod.id,
  workspace_id: workspace.id,
  reference: 'PRJ-1',
  title: '주 작업',
  description: '',
  visibility: 'private',
  access: 'owner',
  archived: false,
  updated_at: '',
  status: 'active',
  is_template: false,
  version: 1,
  starts_on: null,
  ends_on: null,
};
const profile: ItemProfileView = {
  leader: { user_id: 'leader', display_name: '김리더' },
  goal: '',
  scope: '',
  starts_on: null,
  ends_on: null,
  status: 'active',
  details: {},
  members: [{ user_id: 'leader', display_name: '김리더' }],
  groups: [],
  can_edit: true,
  can_manage_members: true,
  version: 1,
};

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'primary-user';
  vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
  vi.spyOn(api, 'item').mockResolvedValue(item);
  vi.spyOn(api, 'module').mockResolvedValue(mod);
  vi.spyOn(api, 'itemDocuments').mockResolvedValue([]);
  vi.spyOn(api, 'itemProfile').mockResolvedValue(profile);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  sessionStorage.clear();
  localStorage.clear();
  resetRoute();
  me.userId = '';
});

async function mount(current = item, space = workspace) {
  vi.mocked(api.item).mockResolvedValue(current);
  render(ItemScreen, { workspace: space, itemId: current.id, workspaces: [space] });
  await screen.findByRole('heading', { name: current.title });
  return screen.findByRole('button', { name: /^항목 정보/ });
}

it('starts with a summary and keeps unsaved title edits visible when collapse is requested', async () => {
  const toggle = await mount();
  expect(toggle).toHaveAttribute('aria-expanded', 'false');
  await waitFor(() => expect(toggle).toHaveTextContent('진행 · 김리더 · 멤버 1명 · 비공개'));
  expect(screen.queryByRole('form', { name: '항목 정보' })).toBeNull();
  await fireEvent.click(toggle);
  await fireEvent.input(screen.getByLabelText('제목', { exact: true }), {
    target: { value: '편집 중' },
  });
  await fireEvent.click(toggle);
  expect(toggle).toHaveAttribute('aria-expanded', 'true');
  expect(screen.getByLabelText('제목', { exact: true })).toHaveValue('편집 중');
  await fireEvent.input(screen.getByLabelText('제목', { exact: true }), {
    target: { value: item.title },
  });
  await fireEvent.click(toggle);
  expect(toggle).toHaveAttribute('aria-expanded', 'false');
});

it('opens only the first entry after creating an empty item', async () => {
  writeSession(org.key('new-item-info'), item.id);
  const toggle = await mount();
  expect(toggle).toHaveAttribute('aria-expanded', 'true');
  await fireEvent.click(toggle);
  expect(toggle).toHaveAttribute('aria-expanded', 'false');
  cleanup();
  expect(await mount()).toHaveAttribute('aria-expanded', 'false');
});

it('blocks collapse while profile input is unsaved and keeps the input on cancelled navigation', async () => {
  vi.spyOn(work, 'assignable').mockResolvedValue([]);
  const toggle = await mount();
  await fireEvent.click(toggle);
  const panel = await screen.findByRole('region', { name: '정보' });
  await fireEvent.click(await within(panel).findByRole('button', { name: '고치기' }));
  await fireEvent.input(screen.getByLabelText('목표', { exact: true }), {
    target: { value: '목표 편집 중' },
  });
  expect(toggle).toBeDisabled();
  await fireEvent.click(toggle);
  expect(toggle).toHaveAttribute('aria-expanded', 'true');
  await fireEvent.click(within(panel).getByRole('button', { name: '취소' }));
  const confirm = await screen.findByRole('alertdialog');
  await fireEvent.click(within(confirm).getByRole('button', { name: '계속 편집' }));
  expect(screen.getByLabelText('목표', { exact: true })).toHaveValue('목표 편집 중');
  await fireEvent.input(screen.getByLabelText('목표', { exact: true }), { target: { value: '' } });
  expect(toggle).not.toBeDisabled();
});

it('invokes the existing template and archive commands through the menu', async () => {
  const template = vi.spyOn(api, 'setItemTemplate').mockResolvedValue(undefined);
  const archive = vi.spyOn(api, 'archiveItem').mockResolvedValue({ ...item, archived: true });
  await mount();
  await fireEvent.click(screen.getByRole('button', { name: '더 보기' }));
  await fireEvent.click(await screen.findByRole('menuitem', { name: '템플릿으로 표시' }));
  expect(template).toHaveBeenCalledWith(item.id, true);
  await fireEvent.click(screen.getByRole('button', { name: '더 보기' }));
  await fireEvent.click(await screen.findByRole('menuitem', { name: '보관', exact: true }));
  expect(archive).toHaveBeenCalledWith(item.id, true);
  expect(await screen.findByText('보관됨 · 읽기 전용')).toBeVisible();
});

it('keeps unsaved member choices open and confirms leaving with DiscardChangesConfirm', async () => {
  vi.spyOn(api, 'members').mockResolvedValue({
    members: [
      { user_id: 'leader', display_name: '김리더', email: null, role: 'member', is_owner: false },
    ],
    my_role: 'admin',
    i_am_owner: true,
    can_manage_members: true,
    pending_owner: null,
  });
  vi.spyOn(api, 'groups').mockResolvedValue([]);
  const toggle = await mount();
  await fireEvent.click(toggle);
  await fireEvent.click(await screen.findByRole('button', { name: '정하기' }));
  await fireEvent.click(await screen.findByRole('checkbox', { name: '김리더' }));
  expect(toggle).toBeDisabled();
  await fireEvent.click(toggle);
  expect(toggle).toHaveAttribute('aria-expanded', 'true');
  await fireEvent.click(
    screen.getByRole('navigation', { name: '경로' }).querySelector('a') ?? toggle,
  );
  const confirm = await screen.findByRole('alertdialog');
  await fireEvent.click(within(confirm).getByRole('button', { name: '계속 편집' }));
  expect(screen.getByRole('checkbox', { name: '김리더' })).not.toBeChecked();
});

it('keeps the copy dialog and its input mounted when Escape requests discarding edits', async () => {
  vi.spyOn(api, 'modules').mockResolvedValue([mod]);
  await mount();
  await fireEvent.click(screen.getByRole('button', { name: '더 보기' }));
  await fireEvent.click(await screen.findByRole('menuitem', { name: '복제', exact: true }));
  const copy = await screen.findByRole('dialog', { name: '항목 복제' });
  await fireEvent.input(within(copy).getByLabelText('새 제목'), {
    target: { value: '사본 편집 중' },
  });
  await fireEvent.keyDown(copy, { key: 'Escape' });
  const confirm = await screen.findByRole('alertdialog');
  await fireEvent.click(within(confirm).getByRole('button', { name: '계속 편집' }));
  expect(await screen.findByRole('dialog', { name: '항목 복제' })).toBeVisible();
  expect(screen.getByLabelText('새 제목')).toHaveValue('사본 편집 중');
});

it.each([
  ['owner', 'owner', false, false, ['보관', '복제', '템플릿으로 표시', '항목 지우기']],
  ['write', 'write', false, false, ['복제', '템플릿으로 표시']],
  ['read', 'write', false, false, ['복제']],
  ['read', 'read', false, false, []],
  ['owner', 'owner', true, false, ['보관 해제', '복제', '항목 지우기']],
  ['owner', 'owner', false, true, ['복제', '항목 지우기']],
] as const)(
  'preserves menu visibility for item %s / workspace %s / archived %s / module archived %s',
  async (access, workspaceAccess, archived, moduleArchived, expected) => {
    vi.mocked(api.module).mockResolvedValue({ ...mod, archived: moduleArchived });
    await mount({ ...item, access, archived }, { ...workspace, access: workspaceAccess });
    const more = screen.queryByRole('button', { name: '더 보기' });
    if (expected.length === 0) {
      expect(more).toBeNull();
      return;
    }
    if (more === null) throw new Error('missing more menu');
    await fireEvent.click(more);
    const menu = await screen.findByRole('menu');
    expect(
      within(menu)
        .getAllByRole('menuitem')
        .map((el) => el.textContent.trim()),
    ).toEqual(expected);
  },
);
