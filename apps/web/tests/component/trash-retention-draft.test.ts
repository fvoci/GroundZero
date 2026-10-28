import type { Workspace } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api, ApiError, type TrashView } from '$lib/api/client';
import { accountSafety } from '$lib/features/feedback/feedback.svelte';
import TrashScreen from '$lib/features/trash/TrashScreen.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

const workspace: Workspace = {
  access: 'owner',
  id: 'ws-1',
  organization_id: 'org-1',
  name: '연구실',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};
const initial: TrashView = { retention_days: 30, version_retention_days: 90, entries: [] };
const fields = ['trash-retention', 'version-retention'] as const;
type Field = (typeof fields)[number];

function input(field: Field): HTMLElement {
  const element = document.getElementById(field);
  if (element === null) throw new Error(`missing ${field}`);
  return element;
}
async function type(field: Field, value: string): Promise<void> {
  await fireEvent.input(input(field), { target: { value } });
}
async function submit(field: Field): Promise<void> {
  const form = input(field).closest('form');
  if (form === null) throw new Error('missing form');
  await fireEvent.submit(form);
}
async function mount(ws = workspace) {
  const view = render(TrashScreen, { props: { workspace: ws } });
  await vi.waitFor(() => {
    expect(input('trash-retention')).toHaveValue('30');
  });
  return view;
}
function drafts(): string[] {
  return Object.keys(localStorage).filter((key) => key.includes('form-draft:trash-'));
}
function warnsBeforeUnload(): boolean {
  const event = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(event);
  return event.defaultPrevented;
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  localStorage.clear();
  sessionStorage.setItem('fvoci:tab', 'trash-tab-a');
  me.userId = 'user-a';
  router.path = '/w/ws-1/trash';
  vi.spyOn(api, 'trash').mockResolvedValue(initial);
  vi.spyOn(api, 'setTrashRetention').mockResolvedValue(undefined);
  vi.spyOn(api, 'setVersionRetention').mockResolvedValue(undefined);
});
afterEach(() => {
  accountSafety.switching = false;
  cleanup();
  localStorage.clear();
  sessionStorage.removeItem('fvoci:tab');
  me.userId = '';
  vi.restoreAllMocks();
});

it.each(fields)(
  '%s stays on screen and warns without storing or restoring a draft',
  async (field) => {
    await mount();
    await type(field, field === 'trash-retention' ? ' 1e- ' : '');
    expect(warnsBeforeUnload()).toBe(true);
    expect(drafts()).toHaveLength(0);
    cleanup();
    await mount();
    expect(input(field)).toHaveValue(field === 'trash-retention' ? '30' : '90');
    expect(screen.queryByTestId('form-draft-offer')).toBeNull();
    expect(warnsBeforeUnload()).toBe(false);
    expect(api.setTrashRetention).not.toHaveBeenCalled();
    expect(api.setVersionRetention).not.toHaveBeenCalled();
  },
);

describe.each(fields)('%s screen session', (field) => {
  const other: Field = field === 'trash-retention' ? 'version-retention' : 'trash-retention';
  const method = field === 'trash-retention' ? 'setTrashRetention' : 'setVersionRetention';

  it('clears only the saved input and keeps the other field through refresh', async () => {
    await mount();
    await type(field, ' 045 ');
    await type(other, '1e-');
    vi.mocked(api.trash).mockResolvedValue({
      ...initial,
      ...(field === 'trash-retention' ? { retention_days: 45 } : { version_retention_days: 45 }),
    });
    await submit(field);
    await vi.waitFor(() => expect(input(field)).toHaveValue('45'));
    expect(api[method]).toHaveBeenCalledExactlyOnceWith('ws-1', 45);
    expect(input(other)).toHaveValue('1e-');
    expect(warnsBeforeUnload()).toBe(true);
    expect(drafts()).toHaveLength(0);
    await type(other, other === 'trash-retention' ? '30' : '90');
    expect(warnsBeforeUnload()).toBe(false);
  });

  it('keeps a rejected input on screen for an explicit retry', async () => {
    vi.mocked(api[method]).mockRejectedValueOnce(new ApiError(403, 'forbidden', 'denied'));
    await mount();
    await type(field, ' 45 ');
    await submit(field);
    await vi.waitFor(() => expect(input(field)).toHaveAttribute('aria-invalid', 'true'));
    expect(input(field)).toHaveValue(' 45 ');
    expect(warnsBeforeUnload()).toBe(true);
    expect(drafts()).toHaveLength(0);
    await submit(field);
    expect(api[method]).toHaveBeenCalledTimes(2);
  });

  it.each(['success', 'failure'] as const)(
    'ignores a late %s from another account session',
    async (result) => {
      const pending = deferred<undefined>();
      vi.mocked(api[method]).mockReturnValueOnce(pending.promise);
      await mount();
      await type(field, '45');
      await submit(field);
      me.userId = 'user-b';
      await vi.waitFor(() => expect(input(field)).not.toHaveValue('45'));
      await type(field, 'B unfinished');
      if (result === 'success') pending.resolve(undefined);
      else pending.reject(new ApiError(403, 'forbidden', 'denied'));
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(input(field)).toHaveValue('B unfinished');
      expect(input(field)).not.toHaveAttribute('aria-invalid', 'true');
      cleanup();
      me.userId = 'user-a';
      await mount();
      expect(input(field)).toHaveValue(field === 'trash-retention' ? '30' : '90');
      expect(screen.queryByTestId('form-draft-offer')).toBeNull();
    },
  );

  it('enforces the existing 1..3650 range without persisting invalid input', async () => {
    await mount();
    for (const value of ['0', '3651', '1.5', '1e-']) {
      await type(field, value);
      await submit(field);
      expect(api[method]).not.toHaveBeenCalled();
      expect(input(field)).toHaveValue(value);
      expect(input(field)).toHaveAttribute('aria-invalid', 'true');
    }
    for (const value of ['1', '3650']) {
      await type(field, value);
      await submit(field);
      expect(api[method]).toHaveBeenLastCalledWith('ws-1', Number(value));
    }
    expect(drafts()).toHaveLength(0);
  });

  it.each(['pending', 'rejected'] as const)(
    'guards blank input while the server baseline is %s',
    async (loadState) => {
      const load = deferred<TrashView>();
      vi.mocked(api.trash).mockReturnValueOnce(load.promise);
      render(TrashScreen, { props: { workspace } });
      await type(field, '');
      if (loadState === 'rejected') {
        load.reject(new ApiError(503, 'unavailable', 'unavailable'));
        await screen.findByRole('alert');
      }
      expect(warnsBeforeUnload()).toBe(true);
      const move = vi.fn();
      router.confirmLeave(move);
      await screen.findByRole('alertdialog');
      expect(move).not.toHaveBeenCalled();
      await fireEvent.click(screen.getByRole('button', { name: m.common_discard_keep() }));
      expect(input(field)).toHaveValue('');
      expect(drafts()).toHaveLength(0);
    },
  );
});

it('warns on unload and routing, keeps input on cancel, and discards only after confirmation', async () => {
  await mount();
  await type('trash-retention', '1e-');
  await type('version-retention', '');
  expect(warnsBeforeUnload()).toBe(true);
  router.navigate('/tasks');
  await screen.findByRole('alertdialog');
  expect(router.path).toBe('/w/ws-1/trash');
  await fireEvent.click(screen.getByRole('button', { name: m.common_discard_keep() }));
  expect(input('trash-retention')).toHaveValue('1e-');
  expect(input('version-retention')).toHaveValue('');
  router.navigate('/tasks');
  await fireEvent.click(await screen.findByRole('button', { name: m.common_discard_confirm() }));
  await screen.findByRole('alertdialog');
  expect(router.path).toBe('/w/ws-1/trash');
  await fireEvent.click(screen.getByRole('button', { name: m.common_discard_confirm() }));
  await vi.waitFor(() => expect(router.path).toBe('/tasks'));
  expect(drafts()).toHaveLength(0);
});

it('blocks submission during account switching while keeping visible input', async () => {
  await mount();
  await type('trash-retention', '45');
  accountSafety.switching = true;
  await submit('trash-retention');
  expect(api.setTrashRetention).not.toHaveBeenCalled();
  expect(input('trash-retention')).toHaveValue('45');
  expect(drafts()).toHaveLength(0);
  accountSafety.switching = false;
});

it('clears screen input when workspace or permission changes', async () => {
  const view = await mount();
  await type('trash-retention', '1e-');
  await type('version-retention', '');
  await view.rerender({ workspace: { ...workspace, access: 'read' } });
  expect(document.getElementById('trash-retention')).toBeNull();
  await view.rerender({ workspace: { ...workspace, id: 'ws-2' } });
  await vi.waitFor(() => expect(input('trash-retention')).toHaveValue('30'));
  expect(input('version-retention')).toHaveValue('90');
  await view.rerender({ workspace });
  await vi.waitFor(() => expect(input('trash-retention')).toHaveValue('30'));
  expect(input('version-retention')).toHaveValue('90');
  expect(screen.queryByTestId('form-draft-offer')).toBeNull();
});

it('keeps raw input when the same workspace object is refreshed', async () => {
  const view = await mount();
  await type('trash-retention', ' 1e- ');
  await type('version-retention', '');
  await view.rerender({ workspace: { ...workspace, name: '같은 공간 새 객체' } });
  expect(input('trash-retention')).toHaveValue(' 1e- ');
  expect(input('version-retention')).toHaveValue('');
  expect(warnsBeforeUnload()).toBe(true);
  expect(drafts()).toHaveLength(0);
});

it('ignores an old workspace load after the next workspace has been edited', async () => {
  const old = deferred<TrashView>();
  vi.mocked(api.trash).mockReturnValueOnce(old.promise);
  const view = render(TrashScreen, { props: { workspace } });
  await type('trash-retention', 'A unfinished');
  await view.rerender({ workspace: { ...workspace, id: 'ws-2' } });
  await vi.waitFor(() => expect(input('trash-retention')).toHaveValue('30'));
  await type('trash-retention', 'B unfinished');
  await type('version-retention', '');
  old.resolve({ ...initial, retention_days: 7, version_retention_days: 3650 });
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(input('trash-retention')).toHaveValue('B unfinished');
  expect(input('version-retention')).toHaveValue('');
  expect(screen.getByText(m.trash_retention_admin({ days: 30 }))).toBeInTheDocument();
});

it('marks a blank version input clean only after the server confirms permanent retention', async () => {
  const load = deferred<TrashView>();
  vi.mocked(api.trash).mockReturnValueOnce(load.promise);
  render(TrashScreen, { props: { workspace } });
  await type('version-retention', '');
  expect(warnsBeforeUnload()).toBe(true);
  load.resolve({ ...initial, version_retention_days: null });
  await vi.waitFor(() => expect(warnsBeforeUnload()).toBe(false));
  expect(input('version-retention')).toHaveValue('');
  expect(api.setVersionRetention).not.toHaveBeenCalled();
});

it('keeps blank values while the other retention field refreshes the screen', async () => {
  await mount();
  await type('trash-retention', '');
  await type('version-retention', '45');
  vi.mocked(api.trash).mockResolvedValue({ ...initial, version_retention_days: 45 });
  await submit('version-retention');
  await vi.waitFor(() => expect(api.trash).toHaveBeenCalledTimes(2));
  expect(input('trash-retention')).toHaveValue('');
  await type('version-retention', '');
  await type('trash-retention', '45');
  vi.mocked(api.trash).mockResolvedValue({
    ...initial,
    retention_days: 45,
    version_retention_days: 45,
  });
  await submit('trash-retention');
  await vi.waitFor(() => expect(api.trash).toHaveBeenCalledTimes(3));
  expect(input('version-retention')).toHaveValue('');
  expect(drafts()).toHaveLength(0);
});

it.each(['permission', 'workspace', 'unmount'] as const)(
  'ignores a submitted retention response after the %s session changes',
  async (change) => {
    const pending = deferred<undefined>();
    vi.mocked(api.setTrashRetention).mockReturnValueOnce(pending.promise);
    const view = await mount();
    await type('trash-retention', '45');
    await submit('trash-retention');
    if (change === 'permission') {
      await view.rerender({ workspace: { ...workspace, access: 'read' } });
      await view.rerender({ workspace });
    } else if (change === 'workspace') {
      await view.rerender({ workspace: { ...workspace, id: 'ws-2' } });
    } else {
      cleanup();
      await mount();
    }
    await vi.waitFor(() => expect(input('trash-retention')).toHaveValue('30'));
    await type('trash-retention', 'current input');
    const loads = vi.mocked(api.trash).mock.calls.length;
    pending.resolve(undefined);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(input('trash-retention')).toHaveValue('current input');
    expect(input('trash-retention')).not.toHaveAttribute('aria-invalid', 'true');
    expect(api.trash).toHaveBeenCalledTimes(loads);
    expect(drafts()).toHaveLength(0);
  },
);

it.each(fields)('saving %s confirms only its own previously unknown baseline', async (field) => {
  vi.mocked(api.trash).mockReturnValue(new Promise<TrashView>(() => undefined));
  render(TrashScreen, { props: { workspace } });
  await type('trash-retention', '');
  await type('version-retention', '');
  if (field === 'trash-retention') await type(field, '45');
  const method = field === 'trash-retention' ? 'setTrashRetention' : 'setVersionRetention';
  await submit(field);
  await vi.waitFor(() =>
    expect(api[method]).toHaveBeenCalledExactlyOnceWith(
      'ws-1',
      field === 'trash-retention' ? 45 : null,
    ),
  );
  await type(field, '7');
  await type(field, field === 'trash-retention' ? '45' : '');
  expect(warnsBeforeUnload()).toBe(true);
  const move = vi.fn();
  router.confirmLeave(move);
  await screen.findByRole('alertdialog');
  await fireEvent.click(screen.getByRole('button', { name: m.common_discard_confirm() }));
  await vi.waitFor(() => expect(move).toHaveBeenCalledOnce());
  expect(drafts()).toHaveLength(0);
});
