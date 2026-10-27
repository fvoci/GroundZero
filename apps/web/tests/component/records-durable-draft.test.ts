import './fixtures/authenticated';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { ItemView, RecordView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import * as feedback from '$lib/features/feedback/feedback.svelte';
import { me } from '$lib/me.svelte';
import RecordsPanel from '$lib/features/records/RecordsPanel.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

const item: ItemView = {
  id: 'draft-item',
  module_id: 'm1',
  workspace_id: 'ws',
  reference: 'PRJ-1',
  title: '항목',
  description: '',
  visibility: 'workspace',
  access: 'write',
  archived: false,
  updated_at: '',
  status: 'active',
  is_template: false,
  version: 1,
  starts_on: null,
  ends_on: null,
};
const record: RecordView = {
  id: 'record-a',
  kind: 'risk',
  title: '기존 기록',
  occurred_on: null,
  body: '',
  fields: {},
  files: [],
  created_by: 'user-a',
  created_by_name: null,
  updated_at: '',
  version: 3,
  can_edit: true,
  can_delete: true,
};

function input(key: string): HTMLInputElement | HTMLTextAreaElement {
  const value = document.getElementById(`record-risk-${key}`);
  if (value instanceof HTMLInputElement || value instanceof HTMLTextAreaElement) return value;
  throw new Error(`Missing record input ${key}`);
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.spyOn(api, 'recordKinds').mockResolvedValue([
    {
      kind: 'risk',
      label: '위험',
      module_kind: 'project',
      date_label: null,
      fields: [
        { key: 'amount', label: '수량', type: 'number', choices: [] },
        { key: 'notes', label: '노트', type: 'long_text', choices: [] },
      ],
      file_roles: [],
    },
  ]);
  vi.spyOn(api, 'itemRecords').mockResolvedValue([record]);
  vi.spyOn(api, 'itemFiles').mockResolvedValue([]);
  vi.spyOn(api, 'members').mockResolvedValue({
    members: [],
    my_role: 'member',
    can_manage_members: false,
    i_am_owner: false,
    pending_owner: null,
  });
});
afterEach(() => vi.restoreAllMocks());

it("ignores another account's late record success without changing the new input or status", async () => {
  const done = vi.spyOn(feedback, 'notifyDone');
  let finish: ((value: string) => void) | undefined;
  const pending = new Promise<string>((resolve) => {
    finish = resolve;
  });
  vi.spyOn(api, 'createRecord').mockReturnValue(pending);
  render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await fireEvent.click(await screen.findByRole('button', { name: '새 위험' }));
  await fireEvent.input(input('title'), { target: { value: 'user-a input' } });
  await fireEvent.click(screen.getByRole('button', { name: '위험 더하기' }));
  me.userId = 'user-b';
  await waitFor(() => expect(input('title')).toHaveValue(''));
  await fireEvent.input(input('title'), { target: { value: 'user-b input' } });
  if (finish === undefined) throw new Error('missing completion');
  finish(record.id);
  await pending;
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  expect(input('title')).toHaveValue('user-b input');
  // 일반 완료는 토스트다(D-82). 다른 계정의 늦은 성공은 알리지 않는다.
  expect(done).not.toHaveBeenCalled();
});

it('restores raw numeric and long text input without sending, and warns on cancel and browser exit', async () => {
  const create = vi
    .spyOn(api, 'createRecord')
    .mockRejectedValue(new ApiError(403, 'forbidden', 'denied'));
  const view = render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await fireEvent.click(await screen.findByRole('button', { name: '새 위험' }));
  await fireEvent.input(input('title'), { target: { value: '보존할 제목' } });
  await fireEvent.input(input('amount'), { target: { value: '1e' } });
  await fireEvent.input(input('notes'), { target: { value: '긴 입력\n두 번째 줄' } });
  const unload = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(unload);
  expect(unload.defaultPrevented).toBe(true);
  await fireEvent.click(screen.getByRole('button', { name: m.common_cancel() }));
  expect(screen.getByRole('alertdialog')).toBeInTheDocument();
  view.unmount();
  const restored = render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(input('title')).toHaveValue('보존할 제목');
  expect(input('amount')).toHaveValue('1e');
  expect(input('notes')).toHaveValue('긴 입력\n두 번째 줄');
  expect(create).not.toHaveBeenCalled();
  await fireEvent.input(input('amount'), { target: { value: '12' } });
  await fireEvent.click(screen.getByRole('button', { name: '위험 더하기' }));
  await waitFor(() => {
    expect(create).toHaveBeenCalledOnce();
  });
  expect(input('title')).toHaveValue('보존할 제목');
  const requestId = create.mock.calls[0]?.[1];
  restored.unmount();
  render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(create).toHaveBeenCalledTimes(1);
  await fireEvent.click(screen.getByRole('button', { name: '위험 더하기' }));
  await waitFor(() => {
    expect(create).toHaveBeenCalledTimes(2);
  });
  expect(create.mock.calls[1]?.[1]).toBe(requestId);
});

it('restores the edit-start version after a remount and keeps another account isolated', async () => {
  const update = vi
    .spyOn(api, 'updateRecord')
    .mockRejectedValue(new ApiError(403, 'forbidden', 'denied'));
  const view = render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
  await fireEvent.input(input('title'), { target: { value: '개인 입력' } });
  me.userId = 'user-b';
  await waitFor(() => expect(input('title')).toHaveValue(''));
  expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
  me.userId = 'user-a';
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(input('title')).toHaveValue('개인 입력');
  view.unmount();
  vi.mocked(api.itemRecords).mockResolvedValue([{ ...record, version: 8 }]);
  render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(update).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
  await waitFor(() => {
    expect(update).toHaveBeenCalledWith(
      record.id,
      3,
      expect.objectContaining({ title: '개인 입력' }),
    );
  });
});

it('ignores a conflict reload that finishes after the editing account changes', async () => {
  vi.spyOn(api, 'updateRecord').mockRejectedValue(new ApiError(409, 'conflict', 'changed'));
  let finish: ((value: RecordView[]) => void) | undefined;
  const pending = new Promise<RecordView[]>((resolve) => {
    finish = resolve;
  });
  render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
  await fireEvent.input(input('title'), { target: { value: 'user-a edit' } });
  vi.mocked(api.itemRecords).mockReturnValue(pending);
  await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
  await waitFor(() => {
    expect(api.itemRecords).toHaveBeenCalledTimes(2);
  });
  me.userId = 'user-b';
  await waitFor(() => expect(input('title')).toHaveValue(''));
  if (finish === undefined) throw new Error('missing conflict response');
  finish([{ ...record, version: 4, title: 'old account response' }]);
  await pending;
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  expect(screen.queryByText('old account response')).not.toBeInTheDocument();
  expect(input('title')).toHaveValue('');
});
