import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ItemView, RecordView } from '$lib/api/client';
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

function record(id: string, title: string, version: number): RecordView {
  return {
    id,
    kind: 'risk',
    title,
    occurred_on: null,
    body: '',
    fields: {},
    files: [],
    created_by: 'u1',
    created_by_name: null,
    updated_at: '2026-10-19T00:00:00Z',
    version,
    can_edit: true,
    can_delete: true,
  };
}

function mockPanel(...lists: RecordView[][]): void {
  vi.spyOn(api, 'recordKinds').mockResolvedValue([
    {
      kind: 'risk',
      label: '위험',
      module_kind: 'project',
      date_label: null,
      fields: [],
      file_roles: [],
    },
  ]);
  const records = vi.spyOn(api, 'itemRecords');
  for (const list of lists) records.mockResolvedValueOnce(list);
  records.mockResolvedValue(lists.at(-1) ?? []);
  vi.spyOn(api, 'members').mockResolvedValue({
    members: [],
    my_role: 'writer',
    i_am_owner: false,
    pending_owner: null,
  });
  vi.spyOn(api, 'itemFiles').mockResolvedValue([]);
}

/** 제목이 `title`인 기록 카드(고치는 중이면 편집기가 든 같은 줄). */
function row(title: string): HTMLElement {
  const li = screen.getByText(title).closest('li');
  if (li === null) throw new Error(title);
  return li;
}

function titleInput(): HTMLInputElement {
  const el = document.getElementById('record-risk-title');
  if (el instanceof HTMLInputElement) return el;
  throw new Error('record-risk-title');
}

// 감사 W14: 기록 고치기는 고치기를 연 때의 판을 서버에 보내야 한다. 고치는 사이 패널이 목록을 다시 읽으면(다른 기록
// 지우기, Zotero 가져오기, 항목 저장 등) 저장 때의 최신 판을 보내 서버의 409 확인이 꺼지고 다른 사람의 고침을 덮었다.
describe('record edit base version', () => {
  it('sends the version read when editing started, even after the list reloads', async () => {
    await setLocale('ko', { reload: false });
    // 처음: R 판 3과 S. S를 지운 뒤 다시 읽으면 그사이 다른 사람이 R을 고쳐 판 4.
    mockPanel([record('r', '위험 R', 3), record('s', '위험 S', 1)], [record('r', '위험 R', 4)]);
    vi.spyOn(api, 'deleteRecord').mockResolvedValue(undefined);
    const update = vi
      .spyOn(api, 'updateRecord')
      .mockRejectedValue(new ApiError(409, 'conflict', 'the record changed'));
    render(RecordsPanel, { props: { item, kind: 'project', canWrite: true } });

    await screen.findByText('위험 S');
    await fireEvent.click(within(row('위험 R')).getByRole('button', { name: '고치기' }));
    await fireEvent.input(titleInput(), { target: { value: '내가 고친 R' } });

    // 고치는 중에 다른 기록 S를 지운다(목록을 다시 읽는다).
    await fireEvent.click(within(row('위험 S')).getByRole('button', { name: '지우기' }));
    await fireEvent.click(within(row('위험 S')).getByRole('button', { name: '지우기' }));
    await vi.waitFor(() => {
      expect(screen.queryByText('위험 S')).toBeNull();
    });

    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await vi.waitFor(() => {
      expect(update).toHaveBeenCalled();
    });
    expect(update.mock.calls[0]?.[0]).toBe('r');
    expect(update.mock.calls[0]?.[1]).toBe(3);
  });

  it('keeps the typed input on 409 and saves over the newer version only when saved again', async () => {
    await setLocale('ko', { reload: false });
    // 처음 판 3 → 409 뒤 다시 읽으면 판 5 → 다시 저장한 뒤 다시 읽기.
    mockPanel(
      [record('r', '위험 R', 3)],
      [record('r', '남이 고친 R', 5)],
      [record('r', '최종', 6)],
    );
    const update = vi
      .spyOn(api, 'updateRecord')
      .mockRejectedValueOnce(new ApiError(409, 'conflict', 'the record changed'))
      .mockResolvedValue(6);
    render(RecordsPanel, { props: { item, kind: 'project', canWrite: true } });

    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await fireEvent.input(titleInput(), { target: { value: '내가 고친 R' } });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));

    expect(await screen.findByRole('alert')).toHaveTextContent(
      '다른 사람이 이 기록을 먼저 고쳤습니다',
    );
    expect(titleInput().value).toBe('내가 고친 R');

    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await vi.waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]?.[1]).toBe(5);
    expect(update.mock.calls[1]?.[2].title).toBe('내가 고친 R');
    expect(await screen.findByText('최종')).toBeInTheDocument();
  });

  it('closes the editor with a not-found message when the record was deleted meanwhile', async () => {
    await setLocale('ko', { reload: false });
    mockPanel([record('r', '위험 R', 3)], []);
    vi.spyOn(api, 'updateRecord').mockRejectedValue(
      new ApiError(409, 'conflict', 'the record changed'),
    );
    render(RecordsPanel, { props: { item, kind: 'project', canWrite: true } });

    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await fireEvent.input(titleInput(), { target: { value: '내가 고친 R' } });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));

    expect(await screen.findByRole('alert')).toHaveTextContent('없거나 볼 수 없는 대상입니다');
    expect(document.getElementById('record-risk-title')).toBeNull();
  });
});
