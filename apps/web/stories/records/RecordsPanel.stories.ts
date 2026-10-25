import type { Meta, StoryObj } from '@storybook/svelte';
import { screen, spyOn, userEvent } from 'storybook/test';

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

import MainFrame from '../fixtures/MainFrame.svelte';

// 강의 항목의 기록(D-123 질문·복습): 회차 칸은 이 강좌의 회차 가운데 고르고 "번호. 제목"으로 보인다. 복습의 다음 복습일은
// 서버가 알림으로 보낸다(화면은 날짜만 보인다). 실제 서버에는 닿지 않는다(preview.ts의 요청 차단).

const COURSE: ItemView = {
  id: 'story-course',
  module_id: 'story-course-module',
  workspace_id: 'story-ws',
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

const KINDS: RecordKindView[] = [
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

const episode = (id: string, number: number, title: string): EpisodeView => ({
  id,
  number,
  title,
  chapter: '1장',
  duration_seconds: 1200,
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

const COURSE_VIEW: CourseView = {
  details: {
    subject: '수학',
    instructor: '',
    platform: '',
    url: '',
    textbook: '',
    starts_on: null,
    expires_on: null,
    target_on: null,
  },
  episodes: [episode('ep-1', 1, '벡터와 공간'), episode('ep-2', 2, '행렬식과 고유값')],
  can_write: true,
};

const RECORDS: RecordView[] = [
  {
    id: 'rec-q1',
    kind: 'question',
    title: '대칭 행렬의 고유값은 왜 항상 실수인가요?',
    occurred_on: '2026-10-26',
    body: '',
    fields: {
      episode: 'ep-2',
      answer: '스펙트럼 정리 때문입니다. 교재 5.3절을 다시 보세요.',
      state: 'answered',
    },
    files: [],
    created_by: 'u1',
    created_by_name: '김학습',
    updated_at: '2026-10-26T01:00:00Z',
    version: 2,
    can_edit: true,
    can_delete: true,
  },
  {
    id: 'rec-q2',
    kind: 'question',
    title: '내적이 0이면 왜 직교라고 하나요?',
    occurred_on: '2026-10-25',
    body: '',
    fields: { episode: 'ep-1', state: 'open' },
    files: [],
    created_by: 'u1',
    created_by_name: '김학습',
    updated_at: '2026-10-25T01:00:00Z',
    version: 1,
    can_edit: true,
    can_delete: true,
  },
  {
    id: 'rec-r1',
    kind: 'review',
    title: '2강 복습',
    occurred_on: '2026-10-28',
    body: '',
    fields: { episode: 'ep-2', weakness: '행렬식의 성질', rounds: 1 },
    files: [],
    created_by: 'u1',
    created_by_name: '김학습',
    updated_at: '2026-10-26T02:00:00Z',
    version: 1,
    can_edit: true,
    can_delete: true,
  },
];

const meta = {
  title: 'Records/RecordsPanel',
  component: RecordsPanel,
  args: { item: COURSE, kind: 'course', canWrite: true },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: () => {
    spyOn(api, 'recordKinds').mockResolvedValue(KINDS);
    spyOn(api, 'itemRecords').mockResolvedValue(RECORDS);
    spyOn(api, 'members').mockResolvedValue({
      members: [
        { user_id: 'u1', display_name: '김학습', email: null, role: 'member', is_owner: false },
      ],
      my_role: 'member',
      i_am_owner: false,
      can_manage_members: false,
      pending_owner: null,
    });
    spyOn(api, 'course').mockResolvedValue(COURSE_VIEW);
    spyOn(api, 'itemFiles').mockResolvedValue([]);
  },
} satisfies Meta<typeof RecordsPanel>;
export default meta;
type Story = StoryObj<typeof meta>;

/** 강의 항목의 질문 목록: 회차는 "번호. 제목", 답변·상태가 보인다. */
export const CourseQuestions: Story = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  play: async () => {
    await screen.findByText('2. 행렬식과 고유값');
  },
};

export const CourseQuestionsDark: Story = {
  globals: { theme: 'dark', fontScale: '100', viewport: { value: 'desktop' } },
  play: async () => {
    await screen.findByText('2. 행렬식과 고유값');
  },
};

/** 새 질문 편집기: 회차 칸은 이 강좌의 회차 가운데 고른다. */
export const NewQuestionEditor: Story = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  play: async () => {
    await userEvent.click(
      await screen.findByRole('button', { name: m.records_new_kind({ label: '질문' }) }),
    );
    await screen.findByLabelText(m.records_field_question_episode());
  },
};

/** 복습 종류를 고른 좁은 화면·큰 글꼴·어두운 테마. */
export const ReviewNarrowDark200: Story = {
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
  play: async () => {
    await userEvent.click(await screen.findByRole('button', { name: /^복습/ }));
    await screen.findByText('2강 복습');
  },
};
