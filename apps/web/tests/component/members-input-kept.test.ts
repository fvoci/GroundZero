import type { MembersView, Workspace } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api, ApiError, NetworkError } from '$lib/api/client';
import MembersScreen from '$lib/features/members/MembersScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { me } from '$lib/me.svelte';

// 멤버 화면의 초대 이메일·새 그룹 이름(D-82, 감사 W30·W35): 요청이 실패하면 입력을 지우지 않고 이유를 칸 옆에
// 보인다. 성공했을 때만 비운다.
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

function describedText(input: HTMLElement): string | undefined {
  const id = input.getAttribute('aria-describedby');
  if (id === null) return undefined;
  return document.getElementById(id)?.textContent.trim();
}

async function submitWith(label: string, value: string): Promise<HTMLElement> {
  const input = await screen.findByLabelText(label);
  await fireEvent.input(input, { target: { value } });
  const form = input.closest('form');
  if (form === null) throw new Error('form');
  await fireEvent.submit(form);
  return input;
}

describe('members screen keeps typed input on failure', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    me.userId = 'u1';
    localStorage.clear();
    vi.spyOn(api, 'members').mockResolvedValue(view);
    vi.spyOn(api, 'groups').mockResolvedValue([]);
    vi.spyOn(api, 'invitations').mockResolvedValue([]);
  });
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    me.userId = '';
    localStorage.clear();
  });

  it('offers invite and group drafts independently after remount without sending them', async () => {
    const invite = vi.spyOn(api, 'invite');
    const create = vi.spyOn(api, 'createGroup');
    const props = { workspace, meId: 'u1', onchanged: () => undefined };
    render(MembersScreen, { props });
    await fireEvent.input(await screen.findByLabelText('이메일'), {
      target: { value: 'draft@example.test' },
    });
    await fireEvent.input(screen.getByLabelText('새 그룹 이름'), {
      target: { value: '남길 그룹' },
    });
    cleanup();
    render(MembersScreen, { props });
    expect(await screen.findByLabelText('이메일')).toHaveValue('');
    const offers = await screen.findAllByTestId('form-draft-offer');
    expect(offers).toHaveLength(2);
    for (const offer of offers)
      await fireEvent.click(within(offer).getByRole('button', { name: '입력 되살리기' }));
    expect(screen.getByLabelText('이메일')).toHaveValue('draft@example.test');
    expect(screen.getByLabelText('새 그룹 이름')).toHaveValue('남길 그룹');
    expect(invite).not.toHaveBeenCalled();
    expect(create).not.toHaveBeenCalled();
    const unload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(true);
  });

  it('keeps the invite email and shows the reason beside it when the invite fails', async () => {
    const invite = vi
      .spyOn(api, 'invite')
      .mockRejectedValueOnce(new ApiError(400, 'bad_request', 'email is not valid'))
      .mockRejectedValueOnce(new NetworkError('Failed to fetch'));
    render(MembersScreen, { props: { workspace, meId: 'u1', onchanged: () => undefined } });

    const input = await submitWith('이메일', 'kim@lab');
    await vi.waitFor(() => {
      expect(invite).toHaveBeenCalledWith('ws-1', 'kim@lab', 'member');
    });
    await screen.findByText(/요청을 처리하지 못했습니다/);
    expect(input).toHaveValue('kim@lab');
    expect(input).toHaveAttribute('aria-invalid', 'true');
    expect(describedText(input)).toBe('요청을 처리하지 못했습니다. 입력한 값을 확인해 주세요.');
    expect(screen.getByRole('button', { name: '초대' })).toBeEnabled();

    await submitWith('이메일', 'kim@lab.example');
    await vi.waitFor(() => {
      expect(invite).toHaveBeenCalledTimes(2);
    });
    await vi.waitFor(() => {
      expect(describedText(input)).toMatch(/^요청을 처리하지 못했습니다\. 서버에 닿지 않았습니다/);
    });
    expect(input).toHaveValue('kim@lab.example');
  });

  it('ignores an invite completion after its screen has unmounted', async () => {
    let complete: ((value: Awaited<ReturnType<typeof api.invite>>) => void) | undefined;
    const pending = new Promise<Awaited<ReturnType<typeof api.invite>>>((resolve) => {
      complete = resolve;
    });
    const invite = vi.spyOn(api, 'invite').mockReturnValue(pending);
    const changed = vi.fn();
    const rendered = render(MembersScreen, {
      props: { workspace, meId: 'u1', onchanged: changed },
    });
    await submitWith('이메일', 'draft@example.test');
    await vi.waitFor(() => {
      expect(invite).toHaveBeenCalledTimes(1);
    });
    rendered.unmount();
    if (complete === undefined) throw new Error('missing completion');
    complete({
      id: 'inv-1',
      email: 'draft@example.test',
      role: 'member',
      expires_at: '2026-10-28T00:00:00Z',
    });
    await pending;
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(changed).not.toHaveBeenCalled();
    expect(api.members).toHaveBeenCalledTimes(1);
  });

  it('clears the invite email only after the invite succeeds', async () => {
    vi.spyOn(api, 'invite').mockResolvedValue({
      id: 'inv-1',
      email: 'kim@lab.example',
      role: 'member',
      expires_at: '2026-10-28T00:00:00Z',
    });
    render(MembersScreen, { props: { workspace, meId: 'u1', onchanged: () => undefined } });
    const input = await submitWith('이메일', 'kim@lab.example');
    await vi.waitFor(() => {
      expect(input).toHaveValue('');
    });
    expect(input).not.toHaveAttribute('aria-invalid', 'true');
  });

  it('keeps the new group name and shows the conflict beside it', async () => {
    const create = vi
      .spyOn(api, 'createGroup')
      .mockRejectedValue(new ApiError(409, 'invalid_state', 'a group with this name exists'));
    render(MembersScreen, { props: { workspace, meId: 'u1', onchanged: () => undefined } });

    const input = await submitWith('새 그룹 이름', 'Design');
    await vi.waitFor(() => {
      expect(create).toHaveBeenCalledWith('ws-1', 'Design');
    });
    await screen.findByText(/요청을 처리하지 못했습니다/);
    expect(input).toHaveValue('Design');
    expect(input).toHaveAttribute('aria-invalid', 'true');
    expect(describedText(input)).toMatch(/^요청을 처리하지 못했습니다\./);
    expect(screen.getByRole('button', { name: '그룹 만들기' })).toBeEnabled();
  });
});
