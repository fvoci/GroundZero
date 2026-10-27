import type { Meta, StoryObj } from '@storybook/svelte';
import type { TaskStatusDefinition } from '@fvoci/api-client';
import { expect, userEvent, within } from 'storybook/test';
import TaskCsvImport from '$lib/features/tasks/TaskCsvImport.svelte';
import { matchRows, saveImport } from '$lib/features/tasks/csv-import';
import { parseTasksCsv } from '$lib/features/tasks/csv';
import { me } from '$lib/me.svelte';
import { org } from '$lib/org.svelte';
import MainFrame from '../fixtures/MainFrame.svelte';
import { setUpScreen } from '../fixtures/screens';
const statuses: TaskStatusDefinition[] = [
  { id: 'review', name: '검토 대기', color: '', position: 0, category: 'doing', task_count: 0 },
  { id: 'ready', name: 'Ready', color: '', position: 1, category: 'todo', task_count: 0 },
];
const meta = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  title: 'Tasks/TaskCsvImport',
  component: TaskCsvImport,
  args: { itemId: 'csv-item', statuses, onimported: () => Promise.resolve() },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: () => {
    const restore = setUpScreen('/csv');
    const key = org.key('csv-import:csv-item');
    const text = '저장한 업무\n응답을 기다리던 업무';
    const steps = matchRows(parseTasksCsv(text, [], statuses, me.maxTitleChars).rows, []);
    if (steps[0] !== undefined) steps[0].progress.applied = steps[0].key;
    saveImport(
      key,
      text,
      steps.map((s) => s.progress),
    );
    return () => {
      saveImport(key, '', []);
      restore();
    };
  },
} satisfies Meta<typeof TaskCsvImport>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Reentered: Story = {
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(c.getByRole('textbox')).toHaveValue('');
    await userEvent.click(c.getByRole('button', { name: '입력 복원' }));
    await expect(c.getByRole('textbox')).toHaveValue('저장한 업무\n응답을 기다리던 업무');
    await expect(c.getByLabelText(/검토 대기 · Ready/)).toBeVisible();
  },
};
export const NarrowDark200: Story = {
  ...Reentered,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
export const Dark: Story = {
  ...Reentered,
  globals: { theme: 'dark', fontScale: '100', viewport: { value: 'desktop' } },
};
export const Narrow: Story = {
  ...Reentered,
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'narrow375' } },
};
export const LargeText: Story = {
  ...Reentered,
  globals: { theme: 'light', fontScale: '200', viewport: { value: 'desktop' } },
};
