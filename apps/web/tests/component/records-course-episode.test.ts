import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type {
  CourseView,
  EpisodeView,
  ItemView,
  RecordKindView,
  RecordView,
} from '$lib/api/client';
import { api } from '$lib/api/client';
import RecordsPanel from '$lib/features/records/RecordsPanel.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import './fixtures/authenticated';

// D-123: 강의 항목의 질문·복습 기록은 그 강좌의 회차에 붙는다. 회차 칸은 이 강좌의 회차 가운데 고르고, 기록에는
// "번호. 제목"으로 보인다. 지워져 더는 없는 회차는 그렇다고 적는다.

const course: ItemView = {
  id: 'course-1',
  module_id: 'm-course',
  workspace_id: 'ws-1',
  reference: 'LEC-1',
  title: '선형대수',
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

const kinds: RecordKindView[] = [
  {
    kind: 'question',
    label: '질문',
    module_kind: 'course',
    date_label: '질문한 날',
    fields: [
      { key: 'episode', label: '회차', type: 'episode', choices: [] },
      { key: 'answer', label: '답변', type: 'long_text', choices: [] },
      {
        key: 'state',
        label: '상태',
        type: 'choice',
        choices: [
          { value: 'open', label: '미해결' },
          { value: 'answered', label: '답변됨' },
        ],
      },
    ],
    file_roles: [{ value: 'other', label: '첨부' }],
  },
  {
    kind: 'review',
    label: '복습',
    module_kind: 'course',
    date_label: '다음 복습일',
    fields: [
      { key: 'episode', label: '회차', type: 'episode', choices: [] },
      { key: 'weakness', label: '취약 영역', type: 'long_text', choices: [] },
      { key: 'rounds', label: '반복 횟수', type: 'number', choices: [] },
    ],
    file_roles: [],
  },
];

function record(id: string, kind: string, episode: string, title: string): RecordView {
  return {
    id,
    kind,
    title,
    occurred_on: '2026-10-26',
    body: '',
    fields: { episode, state: 'open' },
    files: [],
    created_by: 'u1',
    created_by_name: '나',
    updated_at: '2026-10-26T00:00:00Z',
    version: 1,
    can_edit: true,
    can_delete: true,
  };
}

const episode = (id: string, number: number, title: string): EpisodeView => ({
  id,
  number,
  title,
  chapter: '',
  duration_seconds: 0,
  url: '',
  materials: '',
  planned_on: null,
  mine: {
    status: 'not_started',
    planned_on: null,
    completed_on: null,
    watch_count: 0,
    last_position_seconds: null,
    note: '',
  },
  version: 1,
});

const courseView: CourseView = {
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
  episodes: [episode('ep-1', 1, '벡터'), episode('ep-2', 2, '행렬')],
  can_write: true,
};

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.spyOn(api, 'recordKinds').mockResolvedValue(kinds);
  vi.spyOn(api, 'members').mockResolvedValue({
    members: [],
    my_role: 'member',
    i_am_owner: false,
    can_manage_members: false,
    pending_owner: null,
  });
  vi.spyOn(api, 'itemFiles').mockResolvedValue([]);
  vi.spyOn(api, 'course').mockResolvedValue(courseView);
});
afterEach(() => {
  vi.restoreAllMocks();
});

describe('course question and review records (D-123)', () => {
  it('shows the episode as number and title, and says so when the episode is gone', async () => {
    vi.spyOn(api, 'itemRecords').mockResolvedValue([
      record('r-1', 'question', 'ep-1', '고유값이 왜 실수인가요?'),
      record('r-2', 'question', 'ep-gone', '지워진 회차의 질문'),
    ]);
    render(RecordsPanel, { props: { item: course, kind: 'course', canWrite: true } });
    const first = await screen.findByText('고유값이 왜 실수인가요?');
    const card = first.closest('li') ?? document.body;
    expect(within(card).getByText('1. 벡터')).toBeInTheDocument();
    const gone = screen.getByText('지워진 회차의 질문').closest('li') ?? document.body;
    expect(within(gone).getByText(m.records_hidden_episode())).toBeInTheDocument();
  });

  it('offers the course episodes in the episode field of the editor', async () => {
    vi.spyOn(api, 'itemRecords').mockResolvedValue([]);
    render(RecordsPanel, { props: { item: course, kind: 'course', canWrite: true } });
    await fireEvent.click(
      await screen.findByRole('button', { name: m.records_new_kind({ label: '질문' }) }),
    );
    const select = screen.getByLabelText(m.records_field_question_episode());
    const options = within(select)
      .getAllByRole('option')
      .map((o) => o.textContent.trim());
    expect(options).toEqual([m.records_not_selected(), '1. 벡터', '2. 행렬']);
  });
});

// CodeRabbit(#141): 회차 조회가 실패해도 이미 읽은 기록 목록은 보이고, 회차 이름·선택 실패만 따로 알린다.
describe('course records when the episode list cannot be loaded', () => {
  it('still lists the records and reports the episode failure with a retry', async () => {
    vi.spyOn(api, 'itemRecords').mockResolvedValue([
      record('r-1', 'question', 'ep-1', '고유값이 왜 실수인가요?'),
    ]);
    const courseCall = vi.spyOn(api, 'course').mockRejectedValue(new Error('offline'));
    render(RecordsPanel, { props: { item: course, kind: 'course', canWrite: true } });
    expect(await screen.findByText('고유값이 왜 실수인가요?')).toBeInTheDocument();
    expect(screen.getByRole('alert')).toHaveTextContent(m.records_episodes_load_failed());
    expect(screen.getByText(m.records_hidden_episode())).toBeInTheDocument();
    courseCall.mockResolvedValue(courseView);
    await fireEvent.click(screen.getByRole('button', { name: m.common_retry() }));
    expect(await screen.findByText('1. 벡터')).toBeInTheDocument();
    expect(screen.queryByRole('alert')).toBeNull();
  });
});
