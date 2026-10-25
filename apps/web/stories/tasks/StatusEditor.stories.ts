import type { Meta, StoryObj } from '@storybook/svelte';
import { fn, userEvent, within } from 'storybook/test';
import StatusEditor from '$lib/features/tasks/StatusEditor.svelte';
const meta = {
  title: 'Tasks/StatusEditor',
  component: StatusEditor,
  args: {
    itemId: 'item',
    archived: false,
    onchanged: fn(() => Promise.resolve()),
    statuses: [
      { id: 'one', name: '접수', category: 'todo', color: '', position: 0, task_count: 3 },
      { id: 'two', name: '검토 중', category: 'doing', color: '', position: 1, task_count: 2 },
      { id: 'three', name: '출시 완료', category: 'done', color: '', position: 2, task_count: 1 },
    ],
  },
  play: async ({ canvasElement }) => {
    await userEvent.click(within(canvasElement).getByRole('button', { name: '검토 중 수정' }));
  },
} satisfies Meta<typeof StatusEditor>;

export default meta;
type Story = StoryObj<typeof meta>;

export const LightDesktop: Story = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop', isRotated: false } },
};
export const DarkDesktop: Story = {
  globals: { theme: 'dark', fontScale: '100', viewport: { value: 'desktop', isRotated: false } },
};
export const Light375: Story = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'narrow375', isRotated: false } },
};
export const Dark375: Story = {
  globals: { theme: 'dark', fontScale: '100', viewport: { value: 'narrow375', isRotated: false } },
};
export const Font200: Story = {
  globals: { theme: 'light', fontScale: '200', viewport: { value: 'desktop', isRotated: false } },
};
export const Dark375Font200: Story = {
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375', isRotated: false } },
};
