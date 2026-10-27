import type { Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api, ApiError } from '$lib/api/client';
import * as feedback from '$lib/features/feedback/feedback.svelte';
import DeletedWorkspaces from '$lib/features/trash/DeletedWorkspaces.svelte';
import TrashScreen from '$lib/features/trash/TrashScreen.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

// 보존 기간 입력 오류는 그 칸 옆에 보인다(D-82, 감사 W37). 화면 위 공용 알림이 아니라 칸에 묶인 설명으로.
const workspace: Workspace = {
  access: 'owner',
  id: 'ws-1',
  organization_id: 'org-1',
  name: '연구실',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};

function describedText(input: HTMLElement): string | undefined {
  const id = input.getAttribute('aria-describedby');
  if (id === null) return undefined;
  return document.getElementById(id)?.textContent.trim();
}

describe('trash retention field errors', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    me.userId = 'user-a';
    localStorage.clear();
    vi.spyOn(api, 'trash').mockResolvedValue({
      retention_days: 30,
      version_retention_days: null,
      entries: [],
    });
  });
  afterEach(() => {
    me.userId = '';
    vi.restoreAllMocks();
  });

  it('shows the trash retention range error beside its field', async () => {
    const set = vi.spyOn(api, 'setTrashRetention').mockResolvedValue(undefined);
    render(TrashScreen, { props: { workspace } });
    const input = await screen.findByLabelText('보존 기간(일)');
    await vi.waitFor(() => {
      expect(input).toHaveValue('30');
    });
    await fireEvent.input(input, { target: { value: '0' } });
    const form = input.closest('form');
    if (form === null) throw new Error('form');
    await fireEvent.submit(form);

    expect(set).not.toHaveBeenCalled();
    expect(input).toHaveAttribute('aria-invalid', 'true');
    expect(describedText(input)).toBe('보존 기간은 1~3650일입니다.');
    // 입력은 그대로 남는다.
    expect(input).toHaveValue('0');
  });

  it('shows the version retention range error and a server rejection beside that field', async () => {
    const set = vi
      .spyOn(api, 'setVersionRetention')
      .mockRejectedValueOnce(new ApiError(400, 'bad_request', 'days must be 1..3650'));
    render(TrashScreen, { props: { workspace } });
    const input = await screen.findByLabelText('이전 파일 버전 보존(일, 비우면 영구)');
    const form = input.closest('form');
    if (form === null) throw new Error('form');

    await fireEvent.input(input, { target: { value: '4000' } });
    await fireEvent.submit(form);
    expect(set).not.toHaveBeenCalled();
    expect(input).toHaveAttribute('aria-invalid', 'true');
    expect(describedText(input)).toBe(
      '이전 버전 보존 기간은 1~3650일이거나 비워 둡니다(영구 보존).',
    );

    await fireEvent.input(input, { target: { value: '90' } });
    await fireEvent.submit(form);
    await vi.waitFor(() => {
      expect(describedText(input)).toBe('처리하지 못했습니다. 입력한 값을 확인해 주세요.');
    });
    expect(set).toHaveBeenCalledWith('ws-1', 90);
    expect(input).toHaveValue('90');
  });

  // FC-4: 저장하는 동안 그 폼의 입력·되살리기·단추를 잠근다(D-97). 전에는 잠그지 않아 같은 칸을 연달아 저장하거나 그사이
  // 고칠 수 있었고, 그때 앞 저장의 늦은 실패를 버리면서 실패 문구도 숨겼다. 이제 두 번째 저장은 나가지 않고 실패는 그 칸에
  // 남는다. 두 칸은 따로 저장하므로 다른 칸은 잠그지 않는다.
  it.each([
    ['보존 기간(일)', '이전 파일 버전 보존(일, 비우면 영구)', 'setTrashRetention'],
    ['이전 파일 버전 보존(일, 비우면 영구)', '보존 기간(일)', 'setVersionRetention'],
  ] as const)(
    'locks the %s form while saving and shows its late failure beside the field',
    async (label, other, method) => {
      let reject: (e: unknown) => void = () => undefined;
      const set = vi.spyOn(api, method).mockImplementation(
        () =>
          new Promise<undefined>((_, no: (e: unknown) => void) => {
            reject = no;
          }),
      );
      render(TrashScreen, { props: { workspace } });
      const days = await screen.findByLabelText('보존 기간(일)');
      await vi.waitFor(() => {
        expect(days).toHaveValue('30');
      });
      const input = screen.getByLabelText(label);
      await fireEvent.input(input, { target: { value: '40' } });
      const form = input.closest('form');
      if (form === null) throw new Error('form');
      await fireEvent.submit(form);
      expect(set).toHaveBeenCalledOnce();
      expect(input).toBeDisabled();
      expect(within(form).getByRole('button', { name: '바꾸기' })).toBeDisabled();
      expect(screen.getByLabelText(other)).not.toBeDisabled();
      await fireEvent.submit(form);
      expect(set).toHaveBeenCalledOnce();

      reject(new ApiError(400, 'bad_request', 'days must be 1..3650'));
      await vi.waitFor(() => {
        expect(input).not.toBeDisabled();
      });
      expect(input).toHaveAttribute('aria-invalid', 'true');
      expect(describedText(input)).not.toBeUndefined();
      expect(input).toHaveValue('40');
    },
  );
});

// D-82: 일반 완료는 토스트다. 휴지통 화면의 낭독용 완료 영역(sr-only)과 지운 워크스페이스의 화면 안 완료 문구를 두지 않는다.
describe('trash completions are toasts', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    me.userId = 'user-a';
    localStorage.clear();
  });
  afterEach(() => {
    me.userId = '';
    vi.restoreAllMocks();
  });

  it('reports a changed retention once as a toast and keeps no completion text in the page', async () => {
    vi.spyOn(api, 'trash').mockResolvedValue({
      retention_days: 30,
      version_retention_days: null,
      entries: [],
    });
    vi.spyOn(api, 'setTrashRetention').mockResolvedValue(undefined);
    const done = vi.spyOn(feedback, 'notifyDone');
    render(TrashScreen, { props: { workspace } });
    const input = await screen.findByLabelText('보존 기간(일)');
    await vi.waitFor(() => {
      expect(input).toHaveValue('30');
    });
    await fireEvent.input(input, { target: { value: '45' } });
    const form = input.closest('form');
    if (form === null) throw new Error('form');
    await fireEvent.submit(form);

    await vi.waitFor(() => {
      expect(done).toHaveBeenCalledOnce();
    });
    const text = m.trash_retention_changed({ days: 45 });
    expect(done).toHaveBeenCalledWith(text);
    expect(screen.queryByText(text)).toBeNull();
  });

  it('reports a restored workspace once as a toast and keeps the heading for focus', async () => {
    const gone = {
      id: 'ws-old',
      name: '지운 공간',
      deleted_at: '2026-10-20T00:00:00Z',
      expires_at: '2026-11-19T00:00:00Z',
      expired: false,
    };
    vi.spyOn(api, 'deletedWorkspaces').mockResolvedValueOnce([gone]).mockResolvedValue([]);
    vi.spyOn(api, 'restoreWorkspace').mockResolvedValue(undefined);
    const done = vi.spyOn(feedback, 'notifyDone');
    render(DeletedWorkspaces, { props: { onchanged: () => undefined } });
    await fireEvent.click(
      await screen.findByRole('button', { name: m.trash_restore_named({ name: '지운 공간' }) }),
    );

    await vi.waitFor(() => {
      expect(done).toHaveBeenCalledOnce();
    });
    const text = m.trash_restored({ label: '지운 공간' });
    expect(done).toHaveBeenCalledWith(text);
    expect(screen.queryByText(text)).toBeNull();
    // 목록이 비어도 제목은 남아 초점을 받는다(누른 단추가 사라졌다).
    await vi.waitFor(() => {
      expect(document.activeElement?.id).toBe('deleted-workspaces-heading');
    });
  });
});
