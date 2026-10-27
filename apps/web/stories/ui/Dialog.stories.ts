import type { Meta, StoryObj } from '@storybook/svelte';
import { fn } from 'storybook/test';

import DialogFixture from '../fixtures/DialogFixture.svelte';

// 공통 대화상자(src/lib/ui/dialog, Bits UI). 초점·Escape·내부 스크롤은 브라우저 시험이 단언한다(e2e/ui-dialog.spec.ts,
// representative.spec.ts). 부품은 기본 스토리 하나만 둔다(D-108 개정).
const meta = {
  title: 'UI/Dialog',
  component: DialogFixture,
  args: {
    title: '업무 편집',
    description: '제목을 고친 뒤 저장합니다.',
    onsave: fn(),
  },
} satisfies Meta<typeof DialogFixture>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
