import type { MembersView, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { TaskTimeView, TaskView } from '$lib/api/client';
import { api } from '$lib/api/client';
import MembersScreen from '$lib/features/members/MembersScreen.svelte';
import TaskCalendar from '$lib/features/tasks/TaskCalendar.svelte';
import TaskTimePanel from '$lib/features/time/TaskTimePanel.svelte';
import TimeScreen from '$lib/features/time/TimeScreen.svelte';
import { timer } from '$lib/features/time/timer.svelte';
import TrashScreen from '$lib/features/trash/TrashScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// #135 리뷰에서 나온 상태 처리: 이미 목록이 있어도 실패한 읽기를 다시 시도, 같은 대상의 늦은 응답 버리기, 실패한 첫
// 읽기에서 "불러오는 중"을 남기지 않기, 다시 시도를 누르면 앞 오류를 거두기, 달력 날짜 이름의 요일.
beforeEach(async () => {
  await setLocale('ko', { reload: false });
});
afterEach(() => {
  vi.restoreAllMocks();
});

const workspace: Workspace = {
  id: 'ws-1',
  name: '연구실',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};

const view: MembersView = {
  members: [
    {
      user_id: 'u1',
      display_name: '관리자',
      email: 'admin@example.test',
      role: 'admin',
      is_owner: true,
    },
  ],
  my_role: 'admin',
  i_am_owner: true,
  pending_owner: null,
};

describe('members', () => {
  it('offers a retry when the group list fails after the member list loaded', async () => {
    vi.spyOn(api, 'members').mockResolvedValue(view);
    const groups = vi
      .spyOn(api, 'groups')
      .mockRejectedValueOnce(new Error('offline'))
      .mockResolvedValue([]);
    vi.spyOn(api, 'invitations').mockResolvedValue([]);
    render(MembersScreen, { props: { workspace, meId: 'u1', onchanged: () => undefined } });

    await fireEvent.click(await screen.findByRole('button', { name: '다시 시도' }));
    await waitFor(() => {
      expect(groups).toHaveBeenCalledTimes(2);
    });
    expect(await screen.findByText('그룹이 없습니다.')).toBeInTheDocument();
    expect(screen.queryByRole('alert')).toBeNull();
  });
});

const totals = (mine: number): TaskTimeView => ({
  estimate_minutes: null,
  mine_seconds: mine,
  shared_seconds: 0,
  shared_people: 0,
});

describe('task time panel', () => {
  afterEach(() => {
    timer.entry = null;
  });

  it('keeps the newest total when an older read of the same task answers late', async () => {
    const reads: ((v: TaskTimeView) => void)[] = [];
    vi.spyOn(api, 'taskTime').mockImplementation(
      () =>
        new Promise<TaskTimeView>((r) => {
          reads.push(r);
        }),
    );
    render(TaskTimePanel, { props: { taskId: 't-1' } });
    // 이 업무의 타이머가 끝나면 합계를 다시 읽는다. 처음 읽기가 아직 끝나지 않은 채로 두 번째 읽기를 보낸다.
    timer.entry = {
      id: 'e-1',
      started_at: '2026-10-25T00:00:00Z',
      ended_at: null,
      paused_at: null,
      paused_seconds: 0,
      seconds: 0,
      kind: 'work',
      memo: '',
      target: { kind: 'task', id: 't-1', workspace_id: 'ws-1', label: null },
      version: 1,
    };
    await tick();
    timer.entry = null;
    await waitFor(() => {
      expect(reads).toHaveLength(2);
    });
    const [first, second] = reads;
    if (first === undefined || second === undefined) throw new Error('reads');
    second(totals(3600));
    expect(await screen.findByText(/내 시간 1시간/)).toBeInTheDocument();
    first(totals(60));
    await new Promise((r) => setTimeout(r, 0));
    await tick();
    expect(screen.getByText(/내 시간 1시간/)).toBeInTheDocument();
  });
});

describe('time screen', () => {
  it('drops "loading" after a failed first load and clears the error while retrying', async () => {
    vi.spyOn(api, 'currentTimer').mockResolvedValue(null);
    vi.spyOn(api, 'timeSharing').mockResolvedValue(false);
    vi.spyOn(api, 'timeSummary').mockResolvedValue([]);
    vi.spyOn(api, 'timeEntries')
      .mockRejectedValueOnce(new Error('offline'))
      .mockReturnValue(new Promise(() => undefined));
    render(TimeScreen, { props: { workspaces: [workspace] } });

    const retry = await screen.findByRole('button', { name: '다시 시도' });
    expect(screen.queryAllByText('불러오는 중…')).toHaveLength(0);
    await fireEvent.click(retry);
    expect(screen.queryByText('시간 기록을 불러오지 못했습니다.')).toBeNull();
    expect(screen.getAllByText('불러오는 중…').length).toBeGreaterThan(0);
  });

  it('says there are no entries in an empty week', async () => {
    vi.spyOn(api, 'currentTimer').mockResolvedValue(null);
    vi.spyOn(api, 'timeSharing').mockResolvedValue(false);
    vi.spyOn(api, 'timeSummary').mockResolvedValue([]);
    vi.spyOn(api, 'timeEntries').mockResolvedValue([]);
    render(TimeScreen, { props: { workspaces: [workspace] } });

    expect(await screen.findByText('기록이 없습니다.')).toBeInTheDocument();
  });
});

describe('trash', () => {
  it('clears the error while a retry is running', async () => {
    vi.spyOn(api, 'trash')
      .mockRejectedValueOnce(new Error('offline'))
      .mockReturnValue(new Promise(() => undefined));
    render(TrashScreen, { props: { workspace } });

    await fireEvent.click(await screen.findByRole('button', { name: '다시 시도' }));
    expect(screen.queryByRole('button', { name: '다시 시도' })).toBeNull();
    expect(screen.getByText('불러오는 중…')).toBeInTheDocument();
  });
});

function task(due: string): TaskView {
  return {
    id: '1',
    item_id: 'i1',
    module_id: 'm1',
    workspace_id: 'ws-1',
    parent_id: null,
    reference: 'DEV-1',
    title: '발표 자료',
    description: '',
    status: 'todo',
    priority: 'none',
    start_date: null,
    due_date: due,
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
    move_version: 1,
    completed_at: null,
    access: 'write',
    updated_at: '2026-10-21T00:00:00Z',
  };
}

describe('task calendar', () => {
  it('names each day with its weekday', () => {
    render(TaskCalendar, { props: { tasks: [task('2026-10-25')], onopen: () => undefined } });
    const days = screen.getAllByRole('group').map((g) => g.getAttribute('aria-label') ?? '');
    // 표시 설정의 언어로 요일 이름이 붙는다(한국어 "…요일", 영어 "…day").
    expect(days.some((name) => /요일|day/.test(name))).toBe(true);
  });
});
