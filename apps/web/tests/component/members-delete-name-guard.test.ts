import type { MembersView, Workspace } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api, ApiError } from '$lib/api/client';
import { accountSafety, markAccountSwitch } from '$lib/features/feedback/feedback.svelte';
import MembersScreen from '$lib/features/members/MembersScreen.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

// R3 A: the delete confirmation name is never stored, but leaving the screen with it typed is warned.
const workspace: Workspace = {
  access: 'owner',
  id: 'ws-1',
  organization_id: 'org-1',
  name: '연구실',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};
const view: MembersView = {
  can_manage_members: true,
  members: [
    {
      user_id: 'u1',
      display_name: '관리자',
      email: 'admin@example.test',
      role: 'admin',
      is_owner: true,
    },
  ],
  my_role: 'admin',
  i_am_owner: true,
  pending_owner: null,
};
const props = { workspace, meId: 'u1', onchanged: () => undefined };
const typed = '연구';

function unload(): boolean {
  const event = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(event);
  return event.defaultPrevented;
}
function stored(): string {
  return [localStorage, sessionStorage]
    .map((area) =>
      Object.keys(area)
        .map((key) => `${key}=${area.getItem(key) ?? ''}`)
        .join('\n'),
    )
    .join('\n');
}
async function nameInput(): Promise<HTMLInputElement> {
  const input = await screen.findByLabelText(m.members_delete_name_label({ name: workspace.name }));
  if (!(input instanceof HTMLInputElement)) throw new Error('delete-name');
  return input;
}
const type = (input: HTMLElement, value: string) => fireEvent.input(input, { target: { value } });

describe('workspace delete confirmation name', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    me.userId = 'u1';
    localStorage.clear();
    sessionStorage.clear();
    vi.spyOn(api, 'members').mockResolvedValue(view);
    vi.spyOn(api, 'groups').mockResolvedValue([]);
    vi.spyOn(api, 'invitations').mockResolvedValue([]);
  });
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    router.guard = undefined;
    me.userId = '';
    localStorage.clear();
    sessionStorage.clear();
  });

  it('warns on unload and navigation, keeps on request, and drops it on discard without storing it', async () => {
    render(MembersScreen, { props });
    const input = await nameInput();
    expect(unload()).toBe(false);
    await type(input, typed);
    expect(unload()).toBe(true);
    expect(stored()).not.toContain(typed);
    const move = vi.fn();
    router.confirmLeave(move);
    await screen.findByRole('alertdialog');
    expect(move).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: m.common_discard_keep() }));
    expect(input).toHaveValue(typed);
    router.confirmLeave(move);
    await fireEvent.click(await screen.findByRole('button', { name: m.common_discard_confirm() }));
    expect(move).toHaveBeenCalledOnce();
    expect(input).toHaveValue('');
    expect(unload()).toBe(false);
    expect(stored()).not.toContain(typed);
  });

  it('never offers the name after a remount', async () => {
    const view1 = render(MembersScreen, { props });
    await type(await nameInput(), typed);
    view1.unmount();
    cleanup();
    render(MembersScreen, { props });
    expect(await nameInput()).toHaveValue('');
    expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
    expect(unload()).toBe(false);
    expect(stored()).not.toContain(typed);
  });

  it('does not warn when the delete succeeds and leaves the screen', async () => {
    // R5 delta F-1: 실제 라우터로 이동을 본다(navigate를 바꿔 끼우면 가드를 거치지 않아 막힌 이동을 놓쳤다).
    window.history.replaceState(null, '', '/w/ws-1/members');
    router.path = '/w/ws-1/members';
    const deleted = vi.spyOn(api, 'deleteWorkspace').mockResolvedValue('2026-11-26T00:00:00Z');
    render(MembersScreen, { props });
    const input = await nameInput();
    await type(input, workspace.name);
    const form = input.closest('form');
    if (form === null) throw new Error('form');
    await fireEvent.submit(form);
    await vi.waitFor(() => {
      expect(deleted).toHaveBeenCalledOnce();
    });
    await vi.waitFor(() => {
      expect(router.path).toBe('/trash');
    });
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(input).toHaveValue('');
    expect(unload()).toBe(false);
    router.navigate('/');
  });

  it('keeps the name and the warning when the delete fails', async () => {
    vi.spyOn(api, 'deleteWorkspace').mockRejectedValue(new ApiError(409, 'invalid_state', 'no'));
    render(MembersScreen, { props });
    const input = await nameInput();
    await type(input, workspace.name);
    const form = input.closest('form');
    if (form === null) throw new Error('form');
    await fireEvent.submit(form);
    await screen.findByText(m.members_delete_failed(), { exact: false });
    expect(input).toHaveValue(workspace.name);
    expect(unload()).toBe(true);
  });

  it('warns during an account-switch reload because the name is never kept, and never shows it to the next account', async () => {
    render(MembersScreen, { props });
    const input = await nameInput();
    await type(input, typed);
    markAccountSwitch();
    expect(unload()).toBe(true);
    expect(stored()).not.toContain(typed);
    accountSafety.switching = false;
    flushSync(() => {
      me.userId = 'u2';
    });
    // The screen re-reads for the new user; the rebuilt field must not inherit the name.
    expect(await nameInput()).toHaveValue('');
    expect(unload()).toBe(false);
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
  });

  it('clears the name when the workspace changes', async () => {
    const rendered = render(MembersScreen, { props });
    await type(await nameInput(), typed);
    await rendered.rerender({ ...props, workspace: { ...workspace, id: 'ws-2', name: '다른 곳' } });
    expect(
      await screen.findByLabelText(m.members_delete_name_label({ name: '다른 곳' })),
    ).toHaveValue('');
    expect(unload()).toBe(false);
  });
});
