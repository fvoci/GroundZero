import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, fn, userEvent, within } from 'storybook/test';

import ChecklistForm from '$lib/features/checklists/ChecklistForm.svelte';
import { formDraftTabKey } from '$lib/features/common/form-draft';
import { me } from '$lib/me.svelte';

const meta = {
  title: 'Checklists/ChecklistForm',
  component: ChecklistForm,
  args: {
    idPrefix: 'story-checklist',
    items: [{ id: 'c1', text: '자료 확인', done: false }],
    canEdit: true,
    onadd: fn().mockResolvedValue(undefined),
    ontoggle: fn().mockResolvedValue(undefined),
    onrename: fn().mockResolvedValue(undefined),
    onremove: fn().mockResolvedValue(undefined),
  },
  beforeEach: () => {
    me.userId = 'story-checklist';
    for (const key of Object.keys(localStorage)) {
      if (key.startsWith('fvoci:story-checklist:')) localStorage.removeItem(key);
    }
  },
} satisfies Meta<typeof ChecklistForm>;
export default meta;
type Story = StoryObj<typeof meta>;

export const LegacyDraftIgnored: Story = {
  beforeEach: () => {
    const key = formDraftTabKey('checklist:story-checklist');
    if (key === undefined) throw new Error('missing story user');
    localStorage.setItem(
      key,
      JSON.stringify({
        savedAt: '2026-10-26T12:00:00Z',
        input: {
          draft: '추가할 항목',
          draftRequest: null,
          renaming: { id: 'c1', text: '자료를 다시 확인', original: '자료 확인' },
          pasteOpen: true,
          pasted: '- [ ] 붙여넣은 항목',
          preview: [{ text: '미리보기에서 고친 항목', done: true }],
          previewIds: {},
        },
      }),
    );
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.queryByTestId('checklist-new-item-draft-offer')).toBeNull();
    await expect(canvas.getByLabelText('새 항목')).toHaveValue('');
  },
};
export const ScreenInput: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.type(canvas.getByLabelText('새 항목'), '추가할 항목');
    await userEvent.click(canvas.getByRole('button', { name: 'Markdown 붙여넣기' }));
    await userEvent.type(canvas.getByLabelText(/붙여넣을 체크리스트/), '- [[ ] 붙여넣은 항목');
    await userEvent.click(canvas.getByRole('button', { name: '항목으로 펼치기' }));
    await userEvent.clear(canvas.getByLabelText('항목 1 내용'));
    await userEvent.type(canvas.getByLabelText('항목 1 내용'), '미리보기에서 고친 항목');
    await expect(canvas.getByLabelText('항목 1 내용')).toHaveValue('미리보기에서 고친 항목');
    await expect(Object.values(localStorage).join('')).not.toContain('미리보기에서 고친 항목');
  },
};
export const NarrowLargeText: Story = {
  ...ScreenInput,
  globals: { theme: 'dark', viewport: { value: 'narrow375' }, fontScale: '200' },
};
