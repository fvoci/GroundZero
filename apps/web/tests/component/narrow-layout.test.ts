import { render, screen } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { CourseView, StudyPlanView, TaskView } from '$lib/api/client';
import { api } from '$lib/api/client';
import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
import CalendarEntryButton from '$lib/features/events/CalendarEntryButton.svelte';
import StudyPanel from '$lib/features/study/StudyPanel.svelte';
import GanttChart from '$lib/features/tasks/GanttChart.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 좁은 화면(375px)에서 실제 브라우저로 확인한 배치 문제의 회귀 시험. jsdom은 배치를 하지 않으므로 그 배치를 정하는
// 클래스·스타일을 확인한다. 화면 확인은 PR의 전후 이미지에 있다.
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const wrapperOf = (id: string): string =>
  document.getElementById(id)?.parentElement?.className ?? '';

describe('new-entry title fields in wrapping forms', () => {
  // 제목 칸만 flex-1(기준 폭 0)이면 줄바꿈 폼에서 글자 하나 폭까지 눌렸다("새 단위 제목"이 한 글자씩 세로로).
  it('keeps a minimum width for the study unit title so the other fields wrap instead', async () => {
    await setLocale('ko', { reload: false });
    const plan: StudyPlanView = { can_write: true, units: [] };
    vi.spyOn(api, 'studyPlan').mockResolvedValue(plan);
    vi.spyOn(api, 'studyParticipants').mockResolvedValue([]);
    render(StudyPanel, { props: { itemId: 'i1' } });
    await screen.findByLabelText('새 단위 제목');
    expect(wrapperOf('study-unit-title')).toMatch(/min-w-\[min\(12rem,100%\)\]/);
  });

  it('keeps a minimum width for the course episode title, and shows long titles whole', async () => {
    await setLocale('ko', { reload: false });
    const view: CourseView = {
      can_write: true,
      details: {
        subject: '',
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
          id: 'e1',
          number: 2,
          title: '질문 만들기와 긴 회차 제목',
          chapter: '기초',
          duration_seconds: 900,
          url: '',
          materials: '',
          planned_on: null,
          mine: {
            status: 'in_progress',
            planned_on: null,
            completed_on: null,
            watch_count: 0,
            last_position_seconds: null,
            note: '',
          },
          version: 1,
        },
      ],
    };
    vi.spyOn(api, 'course').mockResolvedValue(view);
    vi.spyOn(api, 'courseParticipants').mockResolvedValue([]);
    render(CoursePanel, { props: { itemId: 'i1' } });
    const title = await screen.findByText('질문 만들기와 긴 회차 제목');
    expect(title.className).not.toMatch(/\btruncate\b/);
    expect(title.className).toMatch(/\bmin-w-40\b/);
    expect(wrapperOf('episode-title')).toMatch(/min-w-\[min\(12rem,100%\)\]/);
  });
});

describe('calendar agenda entries', () => {
  it('are at least 1.5rem tall below md, where the month grid is hidden', () => {
    render(CalendarEntryButton, {
      props: {
        entry: {
          kind: 'date',
          label: '수강 2. 질문 만들기',
          date: {
            kind: 'episode',
            id: 'd1',
            item_id: 'i1',
            title: '질문 만들기',
            date: '2026-10-24',
            personal: false,
            done: false,
          },
        },
        entryKey: 'k',
        itemTitle: '데이터 입문 강좌',
        onopen: () => undefined,
      },
    });
    const button = screen.getByRole('button');
    expect(button.className).toMatch(/(^|\s)py-1(\s|$)/);
    expect(button.className).toMatch(/(^|\s)md:py-0(\s|$)/);
  });
});

describe('gantt name column', () => {
  const task: TaskView = {
    id: 't1',
    item_id: 'i1',
    module_id: 'm1',
    workspace_id: 'ws-1',
    parent_id: null,
    reference: 'DEVP-13',
    title: '긴 제목 검증용 업무: 칸반 카드와 간트 줄 이름이 좁은 화면에서도 잘리지 않는지',
    description: '',
    status: 'todo',
    priority: 'none',
    start_date: '2026-10-26',
    due_date: '2026-10-29',
    estimate_minutes: null,
    tags: [],
    milestone: false,
    predecessors: [],
    series: null,
    occurrence_date: null,
    assignees: [],
    status_category: 'todo',
    checklist_done: 0,
    checklist_total: 0,
    version: 1,
    move_version: 1,
    completed_at: null,
    access: 'write',
    updated_at: '2026-10-24T00:00:00Z',
  };

  // 보이는 폭을 정해 두고 크기 관찰을 바로 알린다.
  const atWidth = (px: number): void => {
    vi.stubGlobal(
      'ResizeObserver',
      class {
        constructor(private readonly cb: () => void) {}
        observe(): void {
          this.cb();
        }
        disconnect = vi.fn();
      },
    );
    vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(px);
  };
  const nameColumn = (): number => {
    const grid = document.querySelector<HTMLElement>('[data-testid="task-gantt"] > div');
    return Number.parseFloat(grid?.style.gridTemplateColumns ?? '');
  };

  it('narrows the name column on a phone-width chart so the day columns stay visible', async () => {
    atWidth(326);
    render(GanttChart, { props: { tasks: [task], onopen: () => undefined } });
    await tick();
    expect(nameColumn()).toBeGreaterThanOrEqual(8);
    expect(nameColumn()).toBeLessThan(10);
  });

  it('keeps the 14rem name column on a wide chart', async () => {
    atWidth(974);
    render(GanttChart, { props: { tasks: [task], onopen: () => undefined } });
    await tick();
    expect(nameColumn()).toBe(14);
  });

  it('shows the whole reference and title on hover when the name is cut short', () => {
    render(GanttChart, { props: { tasks: [task], onopen: () => undefined } });
    const name = document.querySelector<HTMLElement>('[data-task-card="t1"]');
    expect(name?.title).toBe(`DEVP-13 ${task.title}`);
  });
});
