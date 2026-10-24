import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, fn, userEvent, within } from 'storybook/test';

import ClashConfirm from '$lib/features/tasks/ClashConfirm.svelte';
import type { ClashRow } from '$lib/features/tasks/labels';

import { EMOJI, LONG_KO, NO_SPACE } from '../fixtures/text';

// 업무 저장 충돌에서 "내 입력으로 다시 저장"을 고른 뒤 같은 칸을 서로 다르게 바꿨을 때의 확인(D-94).
// 이 story는 확인 UI의 상태(값 비교·바쁨·취소)만 보인다. 충돌에 들어가고 나오는 흐름은 TaskDetail이 맡고
// tests/component의 업무 충돌 시험이 검증한다.
const rows: ClashRow[] = [
  {
    key: 'title',
    label: '제목',
    server: '주간 회의 자료 정리',
    mine: '주간 회의 준비',
    looksSame: false,
  },
  {
    key: 'description',
    label: '설명',
    server: '발표 순서를 정하고 회의록 양식을 공유합니다.',
    mine: '발표 순서를 정합니다.\n회의록 양식은 다음 주에 공유합니다.',
    looksSame: false,
  },
];

const meta = {
  title: 'Tasks/ClashConfirm',
  component: ClashConfirm,
  args: { rows, busy: false, onconfirm: fn(), oncancel: fn() },
} satisfies Meta<typeof ClashConfirm>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 뜨면 제목으로 초점이 가서 읽히고, Tab으로 덮어쓰기·취소 단추에 닿는다. */
export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.getByRole('heading', { name: '다른 사람도 바꾼 칸이 있습니다' }),
    ).toHaveFocus();
    await userEvent.tab();
    await expect(canvas.getByRole('button', { name: '내 값으로 덮어쓰고 저장' })).toHaveFocus();
    await userEvent.tab();
    await expect(canvas.getByRole('button', { name: '취소' })).toHaveFocus();
  },
};

/** Escape는 취소와 같다(아무것도 보내지 않고 입력은 그대로). */
export const EscapeCancels: Story = {
  play: async ({ args }) => {
    await userEvent.keyboard('{Escape}');
    await expect(args.oncancel).toHaveBeenCalledTimes(1);
    await expect(args.onconfirm).not.toHaveBeenCalled();
  },
};

/** 저장 중: 단추를 막고 Escape도 취소하지 않는다. */
export const Busy: Story = {
  args: { busy: true },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole('button', { name: '내 값으로 덮어쓰고 저장' })).toBeDisabled();
    await expect(canvas.getByRole('button', { name: '취소' })).toBeDisabled();
    await userEvent.keyboard('{Escape}');
    await expect(args.oncancel).not.toHaveBeenCalled();
  },
};

/** 긴 값은 줄이지 않고 칸 안에서 스크롤한다. 넘치는 칸만 이름 있는 영역으로 탭 순서에 든다(D-98). */
export const LongValues: Story = {
  args: {
    rows: [
      {
        key: 'title',
        label: '제목',
        server: LONG_KO,
        mine: `${LONG_KO} ${EMOJI}`,
        looksSame: false,
      },
      {
        key: 'description',
        label: '설명',
        server: Array.from({ length: 30 }, (_, i) => `${String(i + 1)}번째 줄: ${NO_SPACE}`).join(
          '\n',
        ),
        mine: '짧은 설명',
        looksSame: false,
      },
    ],
  },
};

/** 서로 다른 값이 같아 보일 때(줄 끝 공백) 그렇다고 알린다. */
export const LooksSame: Story = {
  args: {
    rows: [
      { key: 'title', label: '제목', server: '회의 준비', mine: '회의 준비 ', looksSame: true },
    ],
  },
};
