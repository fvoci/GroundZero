import type { Meta, StoryObj } from '@storybook/svelte';
import { fn } from 'storybook/test';
import FieldConflict from '$lib/features/common/FieldConflict.svelte';
import { taskConflictFields } from '$lib/features/tasks/field-conflict';
import type { TaskInput } from '$lib/features/tasks/model';
import { taskViews } from '../fixtures/synthetic';
import { formOf } from '$lib/features/tasks/form';

const task = taskViews()[0];
if (task === undefined) throw new Error('missing synthetic task');
const base = formOf({ ...task, checklist: [] });
const meta = {
  title: 'Forms/FieldConflict',
  component: FieldConflict<TaskInput>,
  args: {
    fields: taskConflictFields(new Map()),
    base,
    mine: { ...base, title: '내가 준비한 회의', description: '내가 작성한 설명\n다음 줄' },
    server: { ...base, title: '함께 준비하는 회의', description: '다른 탭에서 작성한 설명' },
    choices: {},
    onsave: fn(),
    onedit: fn(),
    oncancel: fn(),
  },
} satisfies Meta<typeof FieldConflict<TaskInput>>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop', isRotated: false } },
};
export const Dark375Font200: Story = {
  args: {
    fields: taskConflictFields(
      new Map([
        ['u1', 'Alex #1111'],
        ['u2', 'Alex #2222'],
      ]),
    ),
    mine: {
      ...base,
      title: '긴제목'.repeat(20),
      description: `${'긴 설명과 줄바꿈\n'.repeat(50)}내 마지막 줄`,
      assignee_ids: ['u1'],
    },
    server: {
      ...base,
      title: '서버제목'.repeat(20),
      description: `${'서버의 전문\n'.repeat(50)}서버 마지막 줄`,
      assignee_ids: ['u2'],
      tags: ['다른 사람이 바꾼 태그'],
    },
  },
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375', isRotated: false } },
};
