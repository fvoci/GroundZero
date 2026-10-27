import type { Meta, StoryObj } from '@storybook/svelte';
import { spyOn } from 'storybook/test';
import { api } from '$lib/api/client';
import AttachmentsPanel from '$lib/features/files/AttachmentsPanel.svelte';
import { ACCESS_DOCUMENT, ACCESS_FILE } from '../fixtures/access';

// 내려받기는 서버의 can_download만 따른다(문서 첨부에는 서버가 allow_copy를 적용한다, D-113).
const meta = {
  title: 'Files/AttachmentsPanel',
  component: AttachmentsPanel,
  args: { target: { kind: 'document', id: ACCESS_DOCUMENT.id }, canWrite: true },
  beforeEach: () => {
    spyOn(api, 'documentFiles').mockResolvedValue([ACCESS_FILE]);
  },
} satisfies Meta<typeof AttachmentsPanel>;
export default meta;
type Story = StoryObj<typeof meta>;
const copyDisabled = (): void => {
  spyOn(api, 'documentFiles').mockResolvedValue([{ ...ACCESS_FILE, can_download: false }]);
};
export const Owner: Story = { globals: { theme: 'light', viewport: { value: 'desktop' } } };
export const WriterCopyDisabled: Story = {
  beforeEach: copyDisabled,
  globals: { theme: 'dark', viewport: { value: 'desktop' } },
};
export const ReadOnlyNarrow: Story = {
  args: { canWrite: false },
  beforeEach: copyDisabled,
  globals: { theme: 'light', viewport: { value: 'narrow375' }, fontScale: '200' },
};
