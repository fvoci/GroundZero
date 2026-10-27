import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';

import type { TaskStatus } from '$lib/api/client';
import { ApiError, NetworkError } from '$lib/api/client';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { itemPath } from '$lib/router.svelte';

import { fixClock } from '../fixtures/clock';
import MainFrame from '../fixtures/MainFrame.svelte';
import { never, setUpScreen, type ScreenApi } from '../fixtures/screens';
import { TASK_ITEM, taskViews, WORKSPACE } from '../fixtures/synthetic';

// 칸반(src/lib/features/tasks/TaskBoard.svelte)의 불러오기·옮기기 저장 상태. 업무는 개발 seed의 "UI 상태 예제" 업무
// 7개(긴 한국어 제목·URL 제목·이모지·숫자·마일스톤)다. 서버 함수만 바꿔 끼우고(stories/fixtures/screens.ts), 옮긴
// 자리·충돌·권한 판단은 제품 코드가 한다. 실제 서버 저장·권한·공동 편집은 브라우저 회귀 시험(cargo xtask browser)이 맡는다.
// "응답 지연"(3초)·"확인 중"(10초)은 화면 타이머라 자동 시험에서 기다리지 않는다. SaveSlow를 브라우저로 열어 확인한다.
const FIRST = taskViews()[0];
if (FIRST === undefined) throw new RangeError('no synthetic task');

function story(overrides: Partial<ScreenApi> = {}): () => () => void {
  return () => {
    const restoreClock = fixClock();
    const restoreScreen = setUpScreen(itemPath(WORKSPACE.id, TASK_ITEM.id), overrides);
    return () => {
      restoreScreen();
      restoreClock();
    };
  };
}

/** 카드 "주간 회의 자료 정리"(진행 중 열 맨 위)에 초점을 두고 Alt+→로 완료 열로 옮긴다. */
async function moveFirstRight(canvasElement: HTMLElement): Promise<HTMLElement> {
  const canvas = within(canvasElement);
  const card = await canvas.findByRole('button', {
    name: new RegExp(`^${FIRST?.reference ?? ''} `),
  });
  card.focus();
  await userEvent.keyboard('{Alt>}{ArrowRight}{/Alt}');
  return card;
}

function lane(canvasElement: HTMLElement, status: TaskStatus): HTMLElement {
  const found = canvasElement.querySelector<HTMLElement>(`[data-lane="${status}"]`);
  if (found === null) throw new Error(`no lane ${status}`);
  return found;
}

const meta = {
  title: 'Tasks/TaskBoard',
  component: TaskBoard,
  args: { item: TASK_ITEM, workspaces: [WORKSPACE], openTaskId: undefined, onstale: fn() },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: story(),
} satisfies Meta<typeof TaskBoard>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 세 열에 seed 업무가 들어간다. 긴 제목·URL 제목은 카드 안에서 줄바꿈되고 넘치지 않는다. */
export const Board: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByTestId('task-board');
    await expect(within(lane(canvasElement, '할 일')).getAllByRole('listitem')).toHaveLength(4);
    await expect(within(lane(canvasElement, '진행 중')).getAllByRole('listitem')).toHaveLength(2);
    await expect(within(lane(canvasElement, '완료')).getAllByRole('listitem')).toHaveLength(1);
  },
};

/** 읽기 권한: 옮기기 메뉴·끌기 손잡이·단축키가 없고 카드는 열기만 한다. */
export const ReadOnly: Story = {
  args: { item: { ...TASK_ITEM, access: 'read' } },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const card = await canvas.findByRole('button', {
      name: new RegExp(`^${FIRST.reference ?? ''} `),
    });
    await expect(card).not.toHaveAttribute('aria-keyshortcuts');
    await expect(canvas.queryByRole('button', { name: `${FIRST.title} 옮기기` })).toBeNull();
  },
};

/** 목록을 기다리는 동안 "불러오는 중…"만 보인다(빈 열로 보이지 않는다). */
export const Loading: Story = {
  beforeEach: story({ tasks: () => never() }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByRole('status')).toHaveTextContent('불러오는 중…');
    await expect(canvas.queryByTestId('task-board')).toBeNull();
  },
};

/** 업무가 없는 항목: 열마다 빈 안내가 보인다. */
export const Empty: Story = {
  beforeEach: story({ tasks: () => Promise.resolve([]) }),
  play: async ({ canvasElement }) => {
    await within(canvasElement).findByTestId('task-board');
    await expect(canvasElement.querySelectorAll('[data-lane-empty]')).toHaveLength(3);
  },
};

/** 목록을 못 읽음(서버 오류): 오류를 알린다. */
export const LoadFailed: Story = {
  beforeEach: story({
    tasks: () =>
      Promise.reject(new ApiError(500, 'internal', '서버 오류가 났습니다. (요청 ID: story-1)')),
  }),
  play: async ({ canvasElement }) => {
    await expect(await within(canvasElement).findByRole('alert')).toHaveTextContent(
      '서버 오류가 났습니다.',
    );
  },
};

/** 옮긴 카드(새 열에 다시 그려진 요소)에 초점이 있는지. */
async function expectFocusOnFirst(canvasElement: HTMLElement): Promise<void> {
  await waitFor(() =>
    expect(canvasElement.querySelector(`[data-task-card="${FIRST?.id ?? ''}"]`)).toHaveFocus(),
  );
}

/**
 * 옮기면 카드가 먼저 옮겨지고 저장되면 확정된다. 완료와 되돌리기 단추는 토스트로 알린다(D-82, 이 이야기 밖의
 * FeedbackHost). 초점은 옮긴 카드에 남는다.
 */
export const MoveSaved: Story = {
  play: async ({ canvasElement }) => {
    await moveFirstRight(canvasElement);
    await waitFor(() => expect(canvasElement.querySelector('[data-save-state]')).toBeNull());
    await expect(within(lane(canvasElement, '완료')).getByText(FIRST.title)).toBeInTheDocument();
    await expectFocusOnFirst(canvasElement);
  },
};

/**
 * 응답이 오지 않음: 카드에 "저장 중…"이 보이고 3초 뒤 "응답 지연"과 다시 확인 단추로 바뀐다(브라우저에서 확인). 기다리는
 * 동안에도 초점은 새 열의 카드에 있다(문서 처음으로 빠지지 않는다).
 */
export const SaveSlow: Story = {
  beforeEach: story({ moveTask: () => never() }),
  play: async ({ canvasElement }) => {
    await moveFirstRight(canvasElement);
    await expect(
      within(canvasElement).getByText('저장 중…', { selector: '[data-save-state] span' }),
    ).toBeInTheDocument();
    await expectFocusOnFirst(canvasElement);
  },
};

/**
 * 연결이 끊김: 결과를 몰라 서버 목록과 대조하고, 옮겨지지 않았으면 카드를 서버 자리로 되돌리고 "저장되지 않음"과
 * 다시 시도·그대로 두기를 보인다.
 */
export const SaveFailed: Story = {
  beforeEach: story({ moveTask: () => Promise.reject(new NetworkError('offline')) }),
  play: async ({ canvasElement }) => {
    await moveFirstRight(canvasElement);
    const canvas = within(canvasElement);
    await expect(await canvas.findByRole('button', { name: '다시 시도' })).toBeInTheDocument();
    await expect(canvas.getByRole('button', { name: '그대로 두기' })).toBeInTheDocument();
    await expect(within(lane(canvasElement, '진행 중')).getByText(FIRST.title)).toBeInTheDocument();
  },
};

/**
 * 409 충돌: 저장되지 않은 이동을 다시 시도했는데 그사이 다른 곳에서 이 업무가 옮겨졌다(조건부 이동 거절, D-89).
 * 덮어쓰지 않고 그렇다고 알린다.
 */
export const RetryConflict: Story = {
  beforeEach: () => {
    let sent = 0;
    return story({
      moveTask: () => {
        sent += 1;
        return Promise.reject(
          sent === 1
            ? new NetworkError('offline')
            : new ApiError(409, 'move_conflict', 'moved elsewhere'),
        );
      },
    })();
  },
  play: async ({ canvasElement }) => {
    await moveFirstRight(canvasElement);
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: '다시 시도' }));
    await waitFor(() =>
      expect(canvas.getByRole('alert')).toHaveTextContent('그사이 다른 곳에서 이 업무가 옮겨져'),
    );
  },
};

/** 권한이 없어짐(403): 이유를 알리고 항목을 다시 읽게 한다(쓰기 단추를 끄기 위해). */
export const Forbidden: Story = {
  beforeEach: story({
    moveTask: () => Promise.reject(new ApiError(403, 'forbidden', 'forbidden')),
  }),
  play: async ({ canvasElement, args }) => {
    await moveFirstRight(canvasElement);
    await expect(await within(canvasElement).findByRole('alert')).toHaveTextContent(
      '고칠 권한이 없어',
    );
    await expect(args.onstale).toHaveBeenCalled();
  },
};

/** 같은 제목의 카드 30장: 번호로 구별되고 열은 세로로 길어진다(열 안 스크롤 없음). */
export const ManySameTitle: Story = {
  beforeEach: story({
    tasks: () =>
      Promise.resolve(
        Array.from({ length: 30 }, (_, i) => ({
          ...FIRST,
          id: `${FIRST.id}-${String(i)}`,
          reference: `DEVU-${String(100 + i)}`,
          status: '할 일',
        })),
      ),
  }),
};
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
