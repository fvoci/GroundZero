import type { EventView } from '$lib/features/events/model';
import type { TimeEntryView } from '$lib/features/time/model';
import type { CourseView, StudyPlanView } from '$lib/api/client';

export const running: TimeEntryView & { stopped_entry_id: string | null } = {
  id: 'timer-1',
  started_at: '2026-10-26T00:00:00Z',
  ended_at: null,
  paused_at: null,
  paused_seconds: 0,
  seconds: 90,
  kind: 'work',
  memo: '',
  target: { kind: 'task', id: 'task-1', label: '검토 업무', workspace_id: 'ws-1' },
  version: 1,
  stopped_entry_id: null,
};
export const course: CourseView = {
  access: 'write',
  details: {
    subject: '선형대수',
    instructor: '',
    platform: '',
    url: '',
    textbook: '',
    starts_on: null,
    expires_on: null,
    target_on: null,
  },
  episodes: [
    {
      id: 'episode-1',
      number: 1,
      title: '벡터',
      chapter: '',
      duration_seconds: 1800,
      url: '',
      materials: '',
      planned_on: null,
      version: 1,
      mine: {
        status: 'not_started',
        planned_on: null,
        completed_on: null,
        watch_count: 0,
        last_position_seconds: null,
        note: '',
      },
    },
  ],
};
export const study: StudyPlanView = {
  access: 'write',
  units: [
    {
      id: 'unit-1',
      number: 1,
      title: '연습 문제',
      chapter: '',
      unit: 'pages',
      amount: 10,
      planned_on: null,
      version: 1,
      mine: {
        status: 'not_started',
        done_amount: 0,
        planned_on: null,
        completed_on: null,
        note: '',
      },
    },
  ],
};
export const recurring: EventView = {
  id: 'event-1',
  item_id: 'item-1',
  workspace_id: 'ws-1',
  kind: 'meeting',
  title: '매일 회의',
  description: '',
  all_day: true,
  start_date: '2026-10-26',
  end_date: '2026-10-26',
  starts_at: null,
  ends_at: null,
  timezone: 'Asia/Seoul',
  recurrence: { freq: 'daily', interval: 1, weekdays: [], until: '2026-10-27', count: null },
  excluded_dates: [],
  version: 1,
  access: 'write',
};
