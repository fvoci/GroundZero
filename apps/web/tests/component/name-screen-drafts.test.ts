import type { Workspace } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import HomeScreen from '$lib/features/workspaces/HomeScreen.svelte';
import WorkspaceScreen from '$lib/features/workspaces/WorkspaceScreen.svelte';
import NotesScreen from '$lib/features/notes/NotesScreen.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

const workspace: Workspace = {
  id: 'ws-1',
  name: 'Space',
  organization_id: 'org-1',
  role: 'admin',
  access: 'owner',
  is_owner: true,
  realtime_enabled: false,
};

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'user-a';
  localStorage.clear();
  vi.spyOn(api, 'documentTemplates').mockResolvedValue([]);
  vi.spyOn(api, 'wikiDocuments').mockResolvedValue([]);
  vi.spyOn(api, 'modules').mockResolvedValue([]);
  vi.spyOn(api, 'myNotes').mockResolvedValue([]);
  vi.spyOn(api, 'myInvitations').mockResolvedValue([]);
  vi.spyOn(api, 'home').mockImplementation(() => new Promise(() => {}));
  vi.spyOn(api, 'myTasks').mockImplementation(() => new Promise(() => {}));
  vi.spyOn(api, 'recentActivity').mockResolvedValue([]);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

async function roundTrip(mount: () => void, id: string): Promise<void> {
  mount();
  const box = document.getElementById(id);
  if (box === null) throw new Error(`missing ${id}`);
  await fireEvent.input(box, { target: { value: '잃지 않을 이름' } });
  cleanup();
  mount();
  await screen.findByTestId('form-draft-offer');
  expect(document.getElementById(id)).toHaveValue('');
  await fireEvent.click(screen.getByRole('button', { name: '입력 되살리기' }));
  expect(document.getElementById(id)).toHaveValue('잃지 않을 이름');
}

it('preserves a workspace name in the selected organization without creating it on restore', async () => {
  const create = vi.spyOn(api, 'createWorkspace');
  await roundTrip(() => {
    render(HomeScreen, { props: { workspaces: [], oncreated: vi.fn(), onjoined: async () => {} } });
  }, 'new-workspace');
  expect(create).not.toHaveBeenCalled();
});

it('preserves a wiki title in its workspace without creating it on restore', async () => {
  const create = vi.spyOn(api, 'createWikiDocument');
  await roundTrip(() => {
    render(WorkspaceScreen, { props: { workspace } });
  }, 'new-document');
  expect(create).not.toHaveBeenCalled();
});

it('preserves a personal note title without creating it on restore', async () => {
  const create = vi.spyOn(api, 'createNote');
  await roundTrip(() => {
    render(NotesScreen);
  }, 'new-note');
  expect(create).not.toHaveBeenCalled();
});
