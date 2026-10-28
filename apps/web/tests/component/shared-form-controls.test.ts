import { workApi as work } from '$lib/features/tasks/api';
import './fixtures/task-statuses';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import ChecklistForm from '$lib/features/checklists/ChecklistForm.svelte';
import CommentsPanel from '$lib/features/comments/CommentsPanel.svelte';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import TaskCsvImport from '$lib/features/tasks/TaskCsvImport.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 기능마다 손으로 꾸민 입력 칸은 공통 Input과 모서리·여백·글자 크기(모바일 16px 미만이면 iOS가 초점 때 확대)·높이·초점
// 표시가 서로 달랐다. 공통 구성요소로 그리는지(data-slot)를 확인한다. 겉모습은 Storybook·개발 앱에서 본다.
const item: ItemView = {
  id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  reference: 'KED-1',
  title: '항목',
  description: '',
  visibility: 'workspace',
  access: 'write',
  archived: false,
  updated_at: '2026-10-21T00:00:00Z',
  status: 'active',
  is_template: false,
  version: 1,
  starts_on: null,
  ends_on: null,
};

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  window.sessionStorage.clear();
  vi.spyOn(work, 'tasks').mockResolvedValue([]);
  vi.spyOn(api, 'savedViews').mockResolvedValue([]);
  vi.spyOn(work, 'syncRecurrences').mockResolvedValue(0);
});
afterEach(() => {
  vi.restoreAllMocks();
  window.sessionStorage.clear();
});

describe('task board fields use the shared controls', () => {
  it('draws the text, tag, date and view-name fields with the shared Input', async () => {
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    await screen.findByTestId('task-board');
    await fireEvent.click(screen.getByRole('button', { name: /필터·정렬/ }));
    const fields = [
      screen.getByPlaceholderText('제목·번호'),
      screen.getByRole('textbox', { name: '태그로 거르기' }),
      screen.getByLabelText('마감 시작일'),
      screen.getByLabelText('마감 끝일'),
      screen.getByRole('textbox', { name: '보기 이름' }),
    ];
    for (const field of fields) {
      expect(field).toHaveAttribute('data-slot', 'input');
      // 입력 칸 테두리는 카드·구분선과 다른 토큰(바탕과 3:1 이상, D-109).
      expect(field).toHaveClass('border-input-border');
    }
  });

  it('draws the status, assignee, priority, sort and saved-view selects with the shared NativeSelect', async () => {
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    await screen.findByTestId('task-board');
    await fireEvent.click(screen.getByRole('button', { name: /필터·정렬/ }));
    const selects = screen.getAllByRole('combobox');
    expect(selects).toHaveLength(5);
    for (const select of selects) {
      expect(select).toHaveAttribute('data-slot', 'native-select');
      expect(select).toHaveClass('border-input-border');
    }
  });
});

describe('comment boxes use the shared Textarea', () => {
  it('draws the new comment and reply boxes with it, and still focuses the reply box when opened', async () => {
    vi.spyOn(api, 'comments').mockResolvedValue([
      {
        id: 'c1',
        parent_id: null,
        body: '첫 댓글',
        author_id: 'u1',
        author_name: 'Test User',
        created_at: '2026-10-21T00:00:00Z',
        edited: false,
        deleted: false,
        resolved: false,
        can_edit: false,
        can_delete: false,
        can_resolve: false,
      },
    ]);
    render(CommentsPanel, { props: { target: { kind: 'document', id: 'doc-1' }, canWrite: true } });
    await screen.findByText('첫 댓글');
    expect(screen.getByRole('textbox', { name: /새 댓글/ })).toHaveAttribute(
      'data-slot',
      'textarea',
    );

    await fireEvent.click(screen.getByRole('button', { name: '답글' }));
    const reply = screen.getByRole('textbox', { name: '답글' });
    expect(reply).toHaveAttribute('data-slot', 'textarea');
    // 손으로 꾸민 textarea의 use:focusOnMount를 구성요소에서는 {@attach}로 넘긴다.
    expect(reply).toHaveFocus();
  });
});

// 리뷰: 공통 Textarea는 내용만큼 자라므로(field-sizing), 긴 CSV·Markdown을 붙여넣는 칸은 높이를 막고 칸 안에서 스크롤한다.
describe('paste boxes stay bounded', () => {
  it('caps the task CSV and checklist paste boxes and scrolls inside them', async () => {
    render(TaskCsvImport, { itemId: 'i1', statuses: [], onimported: () => Promise.resolve() });
    expect(screen.getByLabelText(/한 줄에 한 업무/)).toHaveClass('max-h-96', 'overflow-y-auto');
    render(ChecklistForm, {
      idPrefix: 't',
      items: [],
      canEdit: true,
      onadd: () => Promise.resolve(),
      ontoggle: () => Promise.resolve(),
      onrename: () => Promise.resolve(),
      onremove: () => Promise.resolve(),
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Markdown 붙여넣기' }));
    expect(screen.getByLabelText(/붙여넣을 체크리스트/)).toHaveClass('max-h-96', 'overflow-y-auto');
  });
});
