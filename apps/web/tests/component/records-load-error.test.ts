import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ItemView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import RecordsPanel from '$lib/features/records/RecordsPanel.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

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
      my_role: 'writer',
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

  it('draws nothing when the module has no record kinds', async () => {
    await setLocale('ko', { reload: false });
    const kinds = vi.spyOn(api, 'recordKinds').mockResolvedValue([]);
    vi.spyOn(api, 'itemRecords').mockResolvedValue([]);
    vi.spyOn(api, 'members').mockResolvedValue({
      members: [],
      my_role: 'writer',
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
