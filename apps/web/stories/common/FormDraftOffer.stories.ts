import type { Meta, StoryObj } from '@storybook/svelte';
import { fn } from 'storybook/test';

import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';

const meta = {
  title: 'Common/FormDraftOffer',
  component: FormDraftOffer,
  args: { onrestore: fn(), ondiscard: fn() },
} satisfies Meta<typeof FormDraftOffer>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Light: Story = { globals: { theme: 'light', viewport: { value: 'desktop' } } };
export const Dark: Story = { globals: { theme: 'dark', viewport: { value: 'desktop' } } };
export const NarrowLargeText: Story = {
  globals: { theme: 'dark', viewport: { value: 'narrow375' }, fontScale: '200' },
};
