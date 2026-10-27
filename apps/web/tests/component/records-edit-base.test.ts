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
  version: 1,
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
    my_role: 'member',
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

  // 리뷰: 409 뒤 기준 판을 말없이 최신 판으로 옮기면, 다시 저장만 눌러도 다른 사람이 고친 칸을 보지도 못한 채
  // 덮어썼다. 입력과 고치기를 연 때의 판을 그대로 두고 지금 저장된 내용을 보인 뒤, 덮어쓰기를 골라야만 최신 판으로 보낸다.
  it('keeps the typed input and the edit-start version on 409; overwrites only through the explicit button', async () => {
    await setLocale('ko', { reload: false });
    // 처음 판 3 → 409 뒤 다시 읽으면 남이 고친 판 5 → 덮어쓴 뒤 다시 읽기.
    mockPanel(
      [record('r', '위험 R', 3)],
      [record('r', '남이 고친 R', 5)],
      [record('r', '남이 고친 R', 5)],
      [record('r', '최종', 6)],
    );
    // 서버처럼 판 5를 기준으로 보낼 때만 받아들인다.
    const update = vi
      .spyOn(api, 'updateRecord')
      .mockImplementation((_id: string, version: number) =>
        version === 5
          ? Promise.resolve(6)
          : Promise.reject(new ApiError(409, 'conflict', 'the record changed')),
      );
    render(RecordsPanel, { props: { item, kind: 'project', canWrite: true } });

    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await fireEvent.input(titleInput(), { target: { value: '내가 고친 R' } });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('다른 사람이 이 기록을 먼저 고쳤습니다');
    // 지금 저장된 내용(남이 고친 제목)을 보인다.
    expect(alert).toHaveTextContent('남이 고친 R');
    expect(titleInput().value).toBe('내가 고친 R');

    // 그냥 다시 저장하면 여전히 고치기를 연 때의 판을 보낸다(말없이 덮어쓰지 않는다).
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await vi.waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]?.[1]).toBe(3);
    expect(await screen.findByRole('alert')).toHaveTextContent('남이 고친 R');
    expect(titleInput().value).toBe('내가 고친 R');

    // 덮어쓰기를 골라야 보여 준 최신 판을 기준으로 내 입력을 보낸다.
    await fireEvent.click(screen.getByRole('button', { name: '내 입력으로 덮어쓰기' }));
    await vi.waitFor(() => {
      expect(update).toHaveBeenCalledTimes(3);
    });
    expect(update.mock.calls[2]?.[1]).toBe(5);
    expect(update.mock.calls[2]?.[2].title).toBe('내가 고친 R');
    expect(await screen.findByText('최종')).toBeInTheDocument();
  });

  // 덮어쓰기를 눌렀지만 입력이 틀려(제목 없음) 보내지 못했으면 충돌 안내와 기준 판이 그대로 남아야 한다. 그렇지 않으면
  // 제목을 채운 뒤 그냥 저장만 눌러도 안내 없이 최신 판으로 덮어썼다.
  it('keeps the conflict and the edit-start version when an overwrite is refused by validation', async () => {
    await setLocale('ko', { reload: false });
    mockPanel([record('r', '위험 R', 3)], [record('r', '남이 고친 R', 5)]);
    const update = vi
      .spyOn(api, 'updateRecord')
      .mockRejectedValue(new ApiError(409, 'conflict', 'the record changed'));
    render(RecordsPanel, { props: { item, kind: 'project', canWrite: true } });

    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await fireEvent.input(titleInput(), { target: { value: '내가 고친 R' } });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await screen.findByText('남이 고친 R');

    await fireEvent.input(titleInput(), { target: { value: ' ' } });
    await fireEvent.click(screen.getByRole('button', { name: '내 입력으로 덮어쓰기' }));
    expect(screen.getByText('제목을 적어 주세요.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '내 입력으로 덮어쓰기' })).toBeInTheDocument();

    await fireEvent.input(titleInput(), { target: { value: '내가 고친 R' } });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await vi.waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]?.[1]).toBe(3);
  });

  it('loads the saved version into the form when asked, and then edits from it', async () => {
    await setLocale('ko', { reload: false });
    mockPanel([record('r', '위험 R', 3)], [record('r', '남이 고친 R', 5)]);
    const update = vi
      .spyOn(api, 'updateRecord')
      .mockRejectedValueOnce(new ApiError(409, 'conflict', 'the record changed'))
      .mockResolvedValue(6);
    render(RecordsPanel, { props: { item, kind: 'project', canWrite: true } });

    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await fireEvent.input(titleInput(), { target: { value: '내가 고친 R' } });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await screen.findByRole('alert');

    const load = screen.getByRole('button', { name: '저장된 내용 불러오기' });
    load.focus();
    await fireEvent.click(load);
    expect(titleInput().value).toBe('남이 고친 R');
    expect(screen.queryByRole('alert')).toBeNull();
    // 눌렀던 단추가 사라져도 초점은 제목 칸으로 간다(키보드 사용자가 자리를 잃지 않음).
    await vi.waitFor(() => {
      expect(titleInput()).toHaveFocus();
    });

    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await vi.waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]?.[1]).toBe(5);
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
