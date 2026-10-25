import type { MembersView, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api, ApiError, NetworkError } from '$lib/api/client';
import MembersScreen from '$lib/features/members/MembersScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

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
    vi.spyOn(api, 'members').mockResolvedValue(view);
    vi.spyOn(api, 'groups').mockResolvedValue([]);
    vi.spyOn(api, 'invitations').mockResolvedValue([]);
  });
  afterEach(() => {
    vi.restoreAllMocks();
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
