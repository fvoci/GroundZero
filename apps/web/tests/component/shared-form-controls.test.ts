import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import CommentsPanel from '$lib/features/comments/CommentsPanel.svelte';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
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
  vi.spyOn(api, 'tasks').mockResolvedValue([]);
  vi.spyOn(api, 'savedViews').mockResolvedValue([]);
  vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
});
afterEach(() => {
  vi.restoreAllMocks();
  window.sessionStorage.clear();
});

describe('task board fields use the shared controls', () => {
  it('draws the text, tag, date and view-name fields with the shared Input', async () => {
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    await screen.findByTestId('task-board');
    const fields = [
      screen.getByPlaceholderText('제목·번호'),
      screen.getByRole('textbox', { name: '태그로 거르기' }),
      screen.getByLabelText('마감 시작일'),
      screen.getByLabelText('마감 끝일'),
      screen.getByRole('textbox', { name: '보기 이름' }),
    ];
    for (const field of fields) expect(field).toHaveAttribute('data-slot', 'input');
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
    render(CommentsPanel, { props: { target: { kind: 'document', id: 'doc-1' } } });
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
