import { workApi as work } from '$lib/features/tasks/api';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { api, ApiError } from '$lib/api/client';
import StatusEditor from '$lib/features/tasks/StatusEditor.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { TEST_STATUSES } from './fixtures/task-statuses';
import { me } from '$lib/me.svelte';

beforeEach(async () => {
  me.userId = 'user-a';
  localStorage.clear();
  await setLocale('ko', { reload: false });
});
afterEach(() => vi.restoreAllMocks());

it('preserves status name and color, offers them explicitly, and guards closing', async () => {
  const create = vi.spyOn(work, 'createTaskStatus');
  const props = { itemId: 'i', statuses: TEST_STATUSES, archived: false, onchanged: vi.fn() };
  const first = render(StatusEditor, props);
  await fireEvent.click(screen.getByRole('button', { name: '상태 추가' }));
  await fireEvent.input(screen.getByLabelText('상태 이름'), { target: { value: '공동 검토' } });
  await fireEvent.input(screen.getByLabelText('색 (#rrggbb)'), { target: { value: '#abcdef' } });
  await fireEvent.click(screen.getByRole('button', { name: '취소' }));
  await screen.findByRole('alertdialog');
  await fireEvent.click(screen.getByRole('button', { name: '계속 편집' }));
  first.unmount();
  render(StatusEditor, props);
  await fireEvent.click(screen.getByRole('button', { name: '상태 추가' }));
  await screen.findByTestId('form-draft-offer');
  expect(screen.getByLabelText('상태 이름')).toHaveValue('');
  expect(create).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole('button', { name: '입력 되살리기' }));
  expect(screen.getByLabelText('상태 이름')).toHaveValue('공동 검토');
  expect(screen.getByLabelText('색 (#rrggbb)')).toHaveValue('#abcdef');
  expect(create).not.toHaveBeenCalled();
});

it('adds a status, validates fields, and prevents duplicate requests', async () => {
  const create = vi.spyOn(work, 'createTaskStatus').mockReturnValue(new Promise(() => undefined));
  render(StatusEditor, {
    itemId: 'i',
    statuses: TEST_STATUSES,
    archived: false,
    onchanged: vi.fn(),
  });
  await fireEvent.click(screen.getByRole('button', { name: '상태 추가' }));
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  expect(create).not.toHaveBeenCalled();
  await fireEvent.input(screen.getByLabelText('상태 이름'), { target: { value: '검토 대기' } });
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  expect(create).toHaveBeenCalledExactlyOnceWith(expect.any(String), 'i', '검토 대기', '', 'todo');
});

it('preserves edits on 400 and 409, then saves name, color, category and position', async () => {
  const first = TEST_STATUSES[0];
  if (first === undefined) throw new Error('missing status');
  const update = vi
    .spyOn(work, 'updateTaskStatus')
    .mockRejectedValueOnce(new ApiError(400, 'bad_request', 'color rejected'))
    .mockRejectedValueOnce(new ApiError(409, 'invalid_state', 'the item is archived'))
    .mockRejectedValueOnce(new ApiError(409, 'invalid_state', 'a status with this name exists'))
    .mockResolvedValue(first);
  const changed = vi.fn(() => Promise.resolve());
  render(StatusEditor, {
    itemId: 'i',
    statuses: TEST_STATUSES,
    archived: false,
    onchanged: changed,
  });
  await fireEvent.click(screen.getByRole('button', { name: '할 일 수정' }));
  await fireEvent.input(screen.getByLabelText('상태 이름'), { target: { value: '검토' } });
  await fireEvent.input(screen.getByLabelText('색 (#rrggbb)'), { target: { value: '#abcdef' } });
  await fireEvent.change(screen.getByLabelText('범주'), { target: { value: 'doing' } });
  await fireEvent.input(screen.getByLabelText('순서'), { target: { value: '1.5' } });
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  // D-81: 서버의 영어 진단 문구는 한국어 화면에 보이지 않는다. 번역한 이유만 붙인다.
  const alert = await screen.findByRole('alert');
  expect(alert).toHaveTextContent(`${m.tasks_status_failed()} ${m.common_error_bad_request()}`);
  expect(alert).not.toHaveTextContent('color rejected');
  expect(screen.getByLabelText('상태 이름')).toHaveValue('검토');
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  await waitFor(() => {
    expect(screen.getByRole('alert')).toHaveTextContent(
      `${m.tasks_status_failed()} ${m.common_error_archived()}`,
    );
  });
  expect(screen.getByRole('alert')).not.toHaveTextContent('archived');
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  await waitFor(() => {
    expect(screen.getByRole('alert')).toHaveTextContent(m.tasks_status_name_exists());
  });
  expect(screen.getByRole('alert')).not.toHaveTextContent('a status with this name exists');
  expect(screen.getByLabelText('상태 이름')).toHaveValue('검토');
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  await waitFor(() => {
    expect(changed).toHaveBeenCalledOnce();
  });
  expect(update).toHaveBeenLastCalledWith(expect.any(String), 'i', 's-todo', {
    name: '검토',
    color: '#abcdef',
    category: 'doing',
    position: 1.5,
  });
});

it('requires another status before deletion and sends its id', async () => {
  const remove = vi.spyOn(work, 'deleteTaskStatus').mockResolvedValue(3);
  render(StatusEditor, {
    itemId: 'i',
    statuses: TEST_STATUSES,
    archived: false,
    onchanged: () => Promise.resolve(),
  });
  await fireEvent.click(screen.getByRole('button', { name: '할 일 삭제' }));
  await fireEvent.click(screen.getByRole('button', { name: '업무를 옮기고 삭제' }));
  expect(remove).not.toHaveBeenCalled();
  await fireEvent.change(screen.getByLabelText('업무를 옮길 상태'), {
    target: { value: 's-doing' },
  });
  await fireEvent.click(screen.getByRole('button', { name: '업무를 옮기고 삭제' }));
  expect(remove).toHaveBeenCalledExactlyOnceWith(expect.any(String), 'i', 's-todo', 's-doing');
});

it('disables editing archived items', () => {
  render(StatusEditor, {
    itemId: 'i',
    statuses: TEST_STATUSES,
    archived: true,
    onchanged: vi.fn(),
  });
  for (const button of screen.getAllByRole('button')) expect(button).toBeDisabled();
});

// CodeRabbit(#141): 순서 칸을 비우면 Number('')가 0이라 검증을 지나 열이 맨 앞으로 조용히 옮겨졌다. 빈 순서는 거절한다.
it('rejects an empty position instead of saving 0', async () => {
  const update = vi.spyOn(work, 'updateTaskStatus');
  render(StatusEditor, {
    itemId: 'i',
    statuses: TEST_STATUSES,
    archived: false,
    onchanged: vi.fn(),
  });
  await fireEvent.click(screen.getByRole('button', { name: '할 일 수정' }));
  await fireEvent.input(screen.getByLabelText('순서'), { target: { value: '   ' } });
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  expect(screen.getByRole('status')).toHaveTextContent('이름과 순서를 확인하세요');
  expect(update).not.toHaveBeenCalled();
  await fireEvent.input(screen.getByLabelText('순서'), { target: { value: '2' } });
  update.mockResolvedValue(
    TEST_STATUSES[0] ?? {
      id: '',
      name: '',
      color: '',
      position: 0,
      category: 'todo',
      task_count: 0,
    },
  );
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  await waitFor(() => {
    expect(update).toHaveBeenCalledOnce();
  });
});
