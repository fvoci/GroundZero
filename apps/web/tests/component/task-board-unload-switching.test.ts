import './fixtures/task-statuses';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView, TaskDetailView } from '$lib/api/client';
import { api } from '$lib/api/client';
import { accountSafety } from '$lib/features/feedback/feedback.svelte';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// #88·D-96: 다른 계정으로 바뀌어 이 창을 다시 읽을 때는 저장 안 한 편집의 beforeunload 경고로 다시 읽기를 막지 않는다
// (막고 남으면 앞 계정의 화면과 입력이 새 계정의 세션으로 요청을 보낸다; 입력은 앞 계정의 초안으로 이미 남아 있다). 문서
// 편집기는 이 규칙을 따랐고 업무 보드의 이탈 가드도 같아야 한다. 전환이 아닐 때는 전처럼 경고한다.

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
  updated_at: '2026-10-19T00:00:00Z',
  status: 'active',
  is_template: false,
  version: 1,
  starts_on: null,
  ends_on: null,
};

const detail: TaskDetailView = {
  id: 't-1',
  item_id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  parent_id: null,
  reference: 'KED-1',
  title: '구현',
  description: '',
  status: 'todo',
  status_category: 'todo',
  priority: 'none',
  start_date: null,
  due_date: null,
  estimate_minutes: null,
  tags: [],
  milestone: false,
  predecessors: [],
  series: null,
  occurrence_date: null,
  assignees: [],
  checklist_done: 0,
  checklist_total: 0,
  version: 1,
  move_version: 0,
  completed_at: null,
  access: 'write',
  updated_at: '2026-10-19T00:00:00Z',
  checklist: [],
};

function unloadPrevented(): boolean {
  const event = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(event);
  return event.defaultPrevented;
}

describe('task board unload warning while the account is switching', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    me.userId = 'user-a';
    accountSafety.switching = false;
  });
  afterEach(() => {
    accountSafety.switching = false;
    me.userId = '';
    vi.restoreAllMocks();
    window.localStorage.clear();
  });

  it('warns about unsaved task input normally but not while switching accounts', async () => {
    vi.spyOn(api, 'tasks').mockResolvedValue([detail]);
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
    vi.spyOn(api, 'task').mockResolvedValue(detail);
    vi.spyOn(api, 'assignable').mockResolvedValue([]);
    vi.spyOn(api, 'taskBacklinks').mockResolvedValue([]);
    vi.spyOn(api, 'comments').mockResolvedValue([]);
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: 't-1' } });
    const description = await screen.findByLabelText('설명', { exact: true });
    expect(unloadPrevented()).toBe(false);
    await fireEvent.input(description, { target: { value: '아직 저장하지 않은 설명' } });
    expect(unloadPrevented()).toBe(true);

    accountSafety.switching = true;
    expect(unloadPrevented()).toBe(false);

    accountSafety.switching = false;
    expect(unloadPrevented()).toBe(true);
  });
});
