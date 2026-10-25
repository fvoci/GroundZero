import type { Meta, StoryObj } from '@storybook/svelte';
import { spyOn } from 'storybook/test';
import { api } from '$lib/api/client';
import AttachmentsPanel from '$lib/features/files/AttachmentsPanel.svelte';
import { ACCESS_DOCUMENT, ACCESS_FILE } from '../fixtures/access';

const meta = {
  title: 'Files/AttachmentsPanel',
  component: AttachmentsPanel,
  args: { target: { kind: 'document', id: ACCESS_DOCUMENT.id }, canWrite: true, allowCopy: true },
  beforeEach: () => {
    spyOn(api, 'documentFiles').mockResolvedValue([ACCESS_FILE]);
  },
} satisfies Meta<typeof AttachmentsPanel>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Owner: Story = { globals: { theme: 'light', viewport: { value: 'desktop' } } };
export const WriterCopyDisabled: Story = {
  args: { allowCopy: false },
  globals: { theme: 'dark', viewport: { value: 'desktop' } },
};
export const ReadOnlyNarrow: Story = {
  args: { canWrite: false, allowCopy: false },
  globals: { theme: 'light', viewport: { value: 'narrow375' }, fontScale: '200' },
};
