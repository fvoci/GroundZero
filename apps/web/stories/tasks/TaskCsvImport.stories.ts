import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, within } from 'storybook/test';
import TaskCsvImport from '$lib/features/tasks/TaskCsvImport.svelte';
import { matchRows, saveImport } from '$lib/features/tasks/csv-import';
import { parseTasksCsv } from '$lib/features/tasks/csv';
import { org } from '$lib/org.svelte';
import { writeSession } from '$lib/storage';
import MainFrame from '../fixtures/MainFrame.svelte';
import { setUpScreen } from '../fixtures/screens';
const meta = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  title: 'Tasks/TaskCsvImport',
  component: TaskCsvImport,
  args: { itemId: 'csv-item', onimported: () => Promise.resolve() },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: () => {
    const restore = setUpScreen('/csv');
    const key = org.key('csv-import:csv-item');
    const text = '저장한 업무\n응답을 기다리던 업무';
    const steps = matchRows(parseTasksCsv(text, []).rows, []);
    if (steps[0] !== undefined) steps[0].progress.applied = steps[0].key;
    saveImport(
      key,
      text,
      steps.map((s) => s.progress),
    );
    return () => {
      writeSession(key, null);
      restore();
    };
  },
} satisfies Meta<typeof TaskCsvImport>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Reentered: Story = {
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(c.getByRole('textbox')).toHaveValue('저장한 업무\n응답을 기다리던 업무');
  },
};
export const NarrowDark200: Story = {
  ...Reentered,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
