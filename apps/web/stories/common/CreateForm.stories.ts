import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, fn, userEvent, within } from 'storybook/test';

import { formDraftTabKey } from '$lib/features/common/form-draft';
import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
import { me } from '$lib/me.svelte';

const draftName = 'story-name-create';
const meta = {
  title: 'Common/CreateForm',
  component: CreateForm,
  args: {
    id: 'story-new-name',
    label: '이름',
    placeholder: '새 이름',
    submitLabel: '만들기',
    draftName,
    create: fn().mockResolvedValue(undefined),
  },
  beforeEach: () => {
    me.userId = 'story-user';
    for (const key of Object.keys(localStorage)) {
      if (key.includes(`form-draft:${draftName}:`)) localStorage.removeItem(key);
    }
  },
} satisfies Meta<typeof CreateForm>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Typed: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.type(canvas.getByRole('textbox', { name: '이름' }), '주간 연구 모임');
  },
};

export const RestoreOffer: Story = {
  beforeEach: () => {
    const key = formDraftTabKey(draftName);
    if (key === undefined) throw new Error('missing story user');
    localStorage.setItem(
      key,
      JSON.stringify({
        savedAt: '2026-10-26T12:00:00Z',
        input: { name: '남긴 이름', requestId: null, context: '' },
      }),
    );
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByTestId('form-draft-offer')).toBeVisible();
    await expect(canvas.getByRole('textbox', { name: '이름' })).toHaveValue('');
  },
};

export const Light: Story = {
  ...RestoreOffer,
  globals: { theme: 'light', viewport: { value: 'desktop' } },
};
export const Dark: Story = {
  ...RestoreOffer,
  globals: { theme: 'dark', viewport: { value: 'desktop' } },
};
export const NarrowLargeText: Story = {
  ...RestoreOffer,
  globals: { theme: 'dark', viewport: { value: 'narrow375' }, fontScale: '200' },
};
