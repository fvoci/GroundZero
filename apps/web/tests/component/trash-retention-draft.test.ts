import type { Workspace } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/svelte';
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
async function restore(field: Field): Promise<void> {
  const form = input(field).closest('form');
  if (form === null) throw new Error('missing form');
  await fireEvent.click(
    await within(form).findByRole('button', { name: m.common_draft_restore() }),
  );
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
  cleanup();
  localStorage.clear();
  sessionStorage.removeItem('fvoci:tab');
  me.userId = '';
  vi.restoreAllMocks();
});

it('offers both raw strings after remount without applying or submitting them automatically', async () => {
  await mount();
  await type('trash-retention', ' 1e- ');
  await type('version-retention', '');
  cleanup();
  await mount();
  expect(await screen.findAllByTestId('form-draft-offer')).toHaveLength(2);
  expect(input('trash-retention')).toHaveValue('30');
  expect(input('version-retention')).toHaveValue('90');
  expect(api.setTrashRetention).not.toHaveBeenCalled();
  expect(api.setVersionRetention).not.toHaveBeenCalled();
  await restore('trash-retention');
  await restore('version-retention');
  expect(input('trash-retention')).toHaveValue(' 1e- ');
  expect(input('version-retention')).toHaveValue('');
  expect(api.setTrashRetention).not.toHaveBeenCalled();
  expect(api.setVersionRetention).not.toHaveBeenCalled();
  await submit('trash-retention');
  expect(input('trash-retention')).toHaveAttribute('aria-invalid', 'true');
  expect(api.setTrashRetention).not.toHaveBeenCalled();
  await submit('version-retention');
  expect(api.setVersionRetention).toHaveBeenCalledExactlyOnceWith('ws-1', null);
});

describe.each(fields)('%s submission preservation', (field) => {
  const other: Field = field === 'trash-retention' ? 'version-retention' : 'trash-retention';
  const method = field === 'trash-retention' ? 'setTrashRetention' : 'setVersionRetention';

  it('clears only its successful draft and keeps the other field through refresh', async () => {
    await mount();
    await type(field, ' 045 ');
    await type(other, '1e-');
    cleanup();
    await mount();
    await restore(field);
    await restore(other);
    vi.mocked(api.trash).mockResolvedValue({
      ...initial,
      ...(field === 'trash-retention' ? { retention_days: 45 } : { version_retention_days: 45 }),
    });
    await submit(field);
    await vi.waitFor(() => {
      expect(api[method]).toHaveBeenCalledExactlyOnceWith('ws-1', 45);
    });
    await vi.waitFor(() => {
      expect(input(field)).toHaveValue('45');
    });
    expect(input(other)).toHaveValue('1e-');
    expect(drafts()).toHaveLength(1);
    cleanup();
    // Each field's baseline is still supplied by the API.
    vi.mocked(api.trash).mockResolvedValue(initial);
    await mount();
    expect(await screen.findAllByTestId('form-draft-offer')).toHaveLength(1);
    await restore(other);
    expect(input(other)).toHaveValue('1e-');
  });

  it('retains failed input for explicit restoration and retry', async () => {
    vi.mocked(api[method]).mockRejectedValue(new ApiError(403, 'forbidden', 'denied'));
    await mount();
    await type(field, ' 45 ');
    await submit(field);
    await vi.waitFor(() => {
      expect(input(field)).toHaveAttribute('aria-invalid', 'true');
    });
    expect(input(field)).toHaveValue(' 45 ');
    cleanup();
    await mount();
    await restore(field);
    expect(input(field)).toHaveValue(' 45 ');
    expect(api[method]).toHaveBeenCalledTimes(1);
  });

  it('keeps text entered while a save is pending, including after its refresh and remount', async () => {
    const pending = deferred<undefined>();
    vi.mocked(api[method]).mockReturnValueOnce(pending.promise);
    await mount();
    await type(field, '45');
    await submit(field);
    await type(field, field === 'version-retention' ? '' : '1e-');
    pending.resolve(undefined);
    await vi.waitFor(() => {
      expect(api.trash).toHaveBeenCalledTimes(2);
    });
    expect(input(field)).toHaveValue(field === 'version-retention' ? '' : '1e-');
    cleanup();
    await mount();
    await restore(field);
    expect(input(field)).toHaveValue(field === 'version-retention' ? '' : '1e-');
  });

  it.each(['success', 'failure'] as const)(
    'ignores a late %s after the account changes',
    async (result) => {
      const pending = deferred<undefined>();
      vi.mocked(api[method]).mockReturnValueOnce(pending.promise);
      await mount();
      await type(field, '45');
      await submit(field);
      me.userId = 'user-b';
      await vi.waitFor(() => {
        expect(input(field)).not.toHaveValue('45');
      });
      expect(screen.queryByTestId('form-draft-offer')).toBeNull();
      await type(field, 'B unfinished');
      if (result === 'success') pending.resolve(undefined);
      else pending.reject(new ApiError(403, 'forbidden', 'denied'));
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(input(field)).toHaveValue('B unfinished');
      expect(input(field)).not.toHaveAttribute('aria-invalid', 'true');
      cleanup();
      me.userId = 'user-a';
      await mount();
      await restore(field);
      expect(input(field)).toHaveValue('45');
    },
  );
});

it('isolates workspaces and preserves input across permission loss and restoration', async () => {
  const view = await mount();
  await type('trash-retention', '1e-');
  await type('version-retention', '');
  await view.rerender({ workspace: { ...workspace, access: 'read' } });
  expect(document.getElementById('trash-retention')).toBeNull();
  expect(screen.queryByTestId('form-draft-offer')).toBeNull();
  await view.rerender({ workspace: { ...workspace, id: 'ws-2' } });
  await vi.waitFor(() => {
    expect(input('trash-retention')).toHaveValue('30');
  });
  expect(screen.queryByTestId('form-draft-offer')).toBeNull();
  expect(input('version-retention')).toHaveValue('90');
  await view.rerender({ workspace });
  await restore('trash-retention');
  await restore('version-retention');
  expect(input('trash-retention')).toHaveValue('1e-');
  expect(input('version-retention')).toHaveValue('');
});

it('keeps both tab copies and requires choosing a draft in the second tab', async () => {
  await mount();
  await type('trash-retention', 'tab A');
  cleanup();
  sessionStorage.setItem('fvoci:tab', 'trash-tab-b');
  await mount();
  await screen.findByTestId('form-draft-offer');
  expect(input('trash-retention')).toHaveValue('30');
  await type('trash-retention', 'tab B');
  expect(drafts()).toHaveLength(2);
  cleanup();
  sessionStorage.setItem('fvoci:tab', 'trash-tab-a');
  await mount();
  expect(await screen.findAllByTestId('form-draft-offer')).toHaveLength(2);
  expect(drafts().some((key) => key.endsWith(':tab:trash-tab-b'))).toBe(true);
});

it('warns before unload and routing, cancels without loss, and discards only on confirmation', async () => {
  await mount();
  await type('trash-retention', '1e-');
  await type('version-retention', '');
  const unload = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(unload);
  expect(unload.defaultPrevented).toBe(true);
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
  await vi.waitFor(() => {
    expect(router.path).toBe('/tasks');
  });
  expect(drafts()).toHaveLength(0);
});

it('freezes submission during account switching without dropping the preserved input', async () => {
  await mount();
  await type('trash-retention', '45');
  accountSafety.switching = true;
  await submit('trash-retention');
  expect(api.setTrashRetention).not.toHaveBeenCalled();
  expect(drafts()).toHaveLength(1);
  accountSafety.switching = false;
  cleanup();
  await mount();
  await restore('trash-retention');
  expect(input('trash-retention')).toHaveValue('45');
});

it('keeps empty trash text and empty version text during a same-screen refresh', async () => {
  await mount();
  await type('trash-retention', '');
  await type('version-retention', '45');
  vi.mocked(api.trash).mockResolvedValue({ ...initial, version_retention_days: 45 });
  await submit('version-retention');
  await vi.waitFor(() => {
    expect(api.trash).toHaveBeenCalledTimes(2);
  });
  expect(input('trash-retention')).toHaveValue('');
  await type('version-retention', '');
  await submit('trash-retention');
  expect(api.setTrashRetention).not.toHaveBeenCalled();
  expect(input('trash-retention')).toHaveAttribute('aria-invalid', 'true');
  cleanup();
  await mount();
  await restore('trash-retention');
  await restore('version-retention');
  expect(input('trash-retention')).toHaveValue('');
  expect(input('version-retention')).toHaveValue('');
});

it('ignores an old workspace load after a new workspace has been loaded and edited', async () => {
  const old = deferred<TrashView>();
  vi.mocked(api.trash).mockReturnValueOnce(old.promise);
  const view = render(TrashScreen, { props: { workspace } });
  await type('trash-retention', 'A unfinished');
  await view.rerender({ workspace: { ...workspace, id: 'ws-2' } });
  await vi.waitFor(() => {
    expect(input('trash-retention')).toHaveValue('30');
  });
  await type('trash-retention', 'B unfinished');
  await type('version-retention', '');
  old.resolve({ ...initial, retention_days: 7, version_retention_days: 3650 });
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(input('trash-retention')).toHaveValue('B unfinished');
  expect(input('version-retention')).toHaveValue('');
  expect(screen.getByText(m.trash_retention_admin({ days: 30 }))).toBeInTheDocument();
  await view.rerender({ workspace });
  await restore('trash-retention');
  expect(input('trash-retention')).toHaveValue('A unfinished');
});

it.each(['permission', 'workspace', 'unmount'] as const)(
  'retains a submitted draft when its late success follows %s change',
  async (change) => {
    const pending = deferred<undefined>();
    vi.mocked(api.setTrashRetention).mockReturnValueOnce(pending.promise);
    const view = await mount();
    await type('trash-retention', '45');
    await type('version-retention', '');
    await submit('trash-retention');
    if (change === 'permission')
      await view.rerender({ workspace: { ...workspace, access: 'read' } });
    else if (change === 'workspace') {
      await view.rerender({ workspace: { ...workspace, id: 'ws-2' } });
      await type('trash-retention', 'other workspace');
    } else cleanup();
    pending.resolve(undefined);
    await new Promise((resolve) => setTimeout(resolve, 0));
    if (change === 'workspace') expect(input('trash-retention')).toHaveValue('other workspace');
    cleanup();
    await mount();
    await restore('trash-retention');
    await restore('version-retention');
    expect(input('trash-retention')).toHaveValue('45');
    expect(input('version-retention')).toHaveValue('');
  },
);

it.each(fields)(
  '%s keeps invalid restored values and the original 1..3650 range',
  async (field) => {
    const method = field === 'trash-retention' ? 'setTrashRetention' : 'setVersionRetention';
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
  },
);

it('discards only a selected offer and clears an input that becomes equal to the server', async () => {
  await mount();
  await type('trash-retention', 'unfinished');
  await type('version-retention', '');
  cleanup();
  await mount();
  const form = input('trash-retention').closest('form');
  if (form === null) throw new Error('missing form');
  await fireEvent.click(within(form).getByRole('button', { name: m.common_draft_discard() }));
  expect(screen.getAllByTestId('form-draft-offer')).toHaveLength(1);
  await restore('version-retention');
  expect(input('version-retention')).toHaveValue('');
  await type('version-retention', '90');
  await vi.waitFor(() => {
    expect(drafts()).toHaveLength(0);
  });
  expect(api.setVersionRetention).not.toHaveBeenCalled();
});

it('preserves both strings when the parent replaces the same workspace with a new object', async () => {
  const view = await mount();
  await type('trash-retention', ' 1e- ');
  await type('version-retention', '');
  await view.rerender({ workspace: { ...workspace, name: '동일 ID 새 객체' } });
  expect(input('trash-retention')).toHaveValue(' 1e- ');
  expect(input('version-retention')).toHaveValue('');
  const unload = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(unload);
  expect(unload.defaultPrevented).toBe(true);
  expect(api.setTrashRetention).not.toHaveBeenCalled();
  expect(api.setVersionRetention).not.toHaveBeenCalled();
  cleanup();
  await mount();
  await restore('trash-retention');
  await restore('version-retention');
  expect(input('trash-retention')).toHaveValue(' 1e- ');
  expect(input('version-retention')).toHaveValue('');
  expect(api.setTrashRetention).not.toHaveBeenCalled();
  expect(api.setVersionRetention).not.toHaveBeenCalled();
});

describe.each(fields)('%s unknown server baseline', (field) => {
  it.each(['pending', 'rejected'] as const)(
    'preserves a restored blank draft while the reload is %s',
    async (loadState) => {
      await mount();
      await type(field, '');
      expect(drafts()).toHaveLength(1);
      cleanup();

      const load = deferred<TrashView>();
      vi.mocked(api.trash).mockReturnValueOnce(load.promise);
      render(TrashScreen, { props: { workspace } });
      await screen.findByTestId('form-draft-offer');
      if (loadState === 'rejected') {
        load.reject(new ApiError(503, 'unavailable', 'unavailable'));
        await screen.findByRole('alert');
      }
      await restore(field);
      expect(input(field)).toHaveValue('');
      expect(drafts()).toHaveLength(1);
      expect(warnsBeforeUnload()).toBe(true);
      const move = vi.fn();
      router.confirmLeave(move);
      await screen.findByRole('alertdialog');
      expect(move).not.toHaveBeenCalled();
      await fireEvent.click(screen.getByRole('button', { name: m.common_discard_keep() }));
      expect(drafts()).toHaveLength(1);
      expect(api.setTrashRetention).not.toHaveBeenCalled();
      expect(api.setVersionRetention).not.toHaveBeenCalled();

      cleanup();
      await mount();
      await restore(field);
      expect(input(field)).toHaveValue('');
      expect(drafts()).toHaveLength(1);
      expect(warnsBeforeUnload()).toBe(true);
    },
  );
});

it('cleans a restored blank only after the server confirms null and keeps that baseline for a fresh workspace object', async () => {
  await mount();
  await type('version-retention', '');
  cleanup();
  const load = deferred<TrashView>();
  vi.mocked(api.trash).mockReturnValueOnce(load.promise);
  const view = render(TrashScreen, { props: { workspace } });
  await restore('version-retention');
  expect(drafts()).toHaveLength(1);
  expect(warnsBeforeUnload()).toBe(true);

  load.resolve({ ...initial, version_retention_days: null });
  await vi.waitFor(() => {
    expect(drafts()).toHaveLength(0);
  });
  expect(input('version-retention')).toHaveValue('');
  expect(warnsBeforeUnload()).toBe(false);
  vi.mocked(api.trash).mockReturnValue(new Promise<TrashView>(() => undefined));
  await view.rerender({ workspace: { ...workspace, name: '같은 공간' } });
  await type('version-retention', '7');
  expect(warnsBeforeUnload()).toBe(true);
  await type('version-retention', '');
  expect(drafts()).toHaveLength(0);
  expect(warnsBeforeUnload()).toBe(false);
  expect(api.setVersionRetention).not.toHaveBeenCalled();
});

it.each(fields)(
  'saving %s confirms only its own baseline while the other is unknown',
  async (field) => {
    const other: Field = field === 'trash-retention' ? 'version-retention' : 'trash-retention';
    const method = field === 'trash-retention' ? 'setTrashRetention' : 'setVersionRetention';
    await mount();
    await type('trash-retention', '');
    await type('version-retention', '');
    cleanup();
    vi.mocked(api.trash).mockReturnValue(new Promise<TrashView>(() => undefined));
    const view = render(TrashScreen, { props: { workspace } });
    await restore('trash-retention');
    await restore('version-retention');
    if (field === 'trash-retention') await type(field, '45');
    await submit(field);
    await vi.waitFor(() => {
      expect(api[method]).toHaveBeenCalledExactlyOnceWith(
        'ws-1',
        field === 'trash-retention' ? 45 : null,
      );
      expect(drafts()).toHaveLength(1);
    });
    expect(drafts()[0]).toContain(
      `form-draft:${other === 'trash-retention' ? 'trash-retention' : 'trash-version-retention'}:`,
    );
    expect(input(other)).toHaveValue('');
    expect(warnsBeforeUnload()).toBe(true);
    await view.rerender({ workspace: { ...workspace, name: '같은 공간' } });
    await type(field, '7');
    expect(drafts()).toHaveLength(2);
    await type(field, field === 'trash-retention' ? '45' : '');
    expect(drafts()).toHaveLength(1);
    cleanup();
    vi.mocked(api.trash).mockResolvedValue(initial);
    await mount();
    await restore(other);
    expect(input(other)).toHaveValue('');
    expect(warnsBeforeUnload()).toBe(true);
  },
);

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
