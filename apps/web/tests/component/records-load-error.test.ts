import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ItemView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import RecordsPanel from '$lib/features/records/RecordsPanel.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import './fixtures/authenticated';

afterEach(() => {
  vi.restoreAllMocks();
});

const item: ItemView = {
  id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  reference: 'PRJ-1',
  title: '항목',
  description: '',
  visibility: 'workspace',
  access: 'write',
  archived: false,
  updated_at: '2026-10-19T00:00:00Z',
  status: 'active',
  is_template: false,
  version: 1,
  starts_on: null,
  ends_on: null,
};

// 기록 종류를 처음부터 못 읽으면 종류가 비어 있어도 실패를 알리고 다시 읽을 수 있어야 한다(조용히 사라지지 않게).
// 종류가 정말 없는 모듈(정상 빈 상태)은 아무것도 그리지 않는다.
describe('records panel loading', () => {
  it('shows a first load failure and recovers on retry', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'recordKinds')
      .mockRejectedValueOnce(new ApiError(403, 'forbidden', 'no'))
      .mockResolvedValue([
        {
          kind: 'risk',
          label: '위험',
          module_kind: 'project',
          date_label: null,
          fields: [],
          file_roles: [],
        },
      ]);
    vi.spyOn(api, 'itemRecords').mockResolvedValue([]);
    vi.spyOn(api, 'members').mockResolvedValue({
      members: [],
      my_role: 'member',
      can_manage_members: false,
      i_am_owner: false,
      pending_owner: null,
    });
    render(RecordsPanel, { props: { item, kind: 'project', canWrite: true } });
    expect(await screen.findByRole('alert')).toHaveTextContent('권한이 없습니다');
    expect(screen.getByRole('heading', { name: '기록' })).toBeInTheDocument();

    await fireEvent.click(screen.getByRole('button', { name: '다시 시도' }));
    expect(await screen.findByRole('group', { name: '기록 종류' })).toHaveTextContent('위험');
    expect(screen.queryByRole('alert')).toBeNull();
  });

  // 숫자 칸(활동 시간 등)을 고쳐 저장하면 숫자로 보낸다. 숫자 입력이 폼의 글자 상태와 섞이면 저장 전에 멈췄다.
  it('saves an edited number field', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'recordKinds').mockResolvedValue([
      {
        kind: 'activity',
        label: '활동',
        module_kind: 'hobby',
        date_label: null,
        fields: [{ key: 'minutes', label: '시간(분)', type: 'number', choices: [] }],
        file_roles: [],
      },
    ]);
    vi.spyOn(api, 'itemRecords').mockResolvedValue([
      {
        id: 'r1',
        kind: 'activity',
        title: '잎사귀 연습',
        occurred_on: null,
        body: '',
        fields: { minutes: 30 },
        files: [],
        created_by: 'u1',
        created_by_name: null,
        updated_at: '2026-10-19T00:00:00Z',
        version: 3,
        can_edit: true,
        can_delete: true,
      },
    ]);
    vi.spyOn(api, 'members').mockResolvedValue({
      members: [],
      my_role: 'member',
      can_manage_members: false,
      i_am_owner: false,
      pending_owner: null,
    });
    vi.spyOn(api, 'itemFiles').mockResolvedValue([]);
    const update = vi.spyOn(api, 'updateRecord').mockResolvedValue(4);
    render(RecordsPanel, { props: { item, kind: 'hobby', canWrite: true } });
    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    const minutes = screen.getByLabelText('시간(분)');
    await fireEvent.input(minutes, { target: { value: '45' } });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await vi.waitFor(() => {
      expect(update).toHaveBeenCalledTimes(1);
    });
    expect(update.mock.calls[0]?.[2].fields).toEqual({ minutes: 45 });
  });

  it('draws nothing when the module has no record kinds', async () => {
    await setLocale('ko', { reload: false });
    const kinds = vi.spyOn(api, 'recordKinds').mockResolvedValue([]);
    vi.spyOn(api, 'itemRecords').mockResolvedValue([]);
    vi.spyOn(api, 'members').mockResolvedValue({
      members: [],
      my_role: 'member',
      can_manage_members: false,
      i_am_owner: false,
      pending_owner: null,
    });
    render(RecordsPanel, { props: { item, kind: 'course', canWrite: true } });
    await vi.waitFor(() => {
      expect(kinds).toHaveBeenCalled();
    });
    expect(screen.queryByRole('heading', { name: '기록' })).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
  });
});
