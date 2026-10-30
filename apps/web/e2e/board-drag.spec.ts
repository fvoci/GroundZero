// SR-261-1·D-79: 375px 보드에서 포인터를 가장자리에 멈춰도 다음 열로 계속 스크롤하며 놓을 자리를 갱신한다.
// Chromium의 실제 마우스·터치 입력을 쓰며, 실제 휴대 기기 시험은 아니다.
import type { Page } from '@playwright/test';

import { expect, str, test, unique } from './fixtures';
import { expectViewport, pageOverflows } from './layout';

interface Point {
  x: number;
  y: number;
}

async function dragInput(page: Page, kind: 'mouse' | 'touch') {
  const touch = kind === 'touch' ? await page.context().newCDPSession(page) : undefined;
  let pressed = false;
  return {
    async start(point: Point) {
      if (touch !== undefined) {
        await touch.send('Input.dispatchTouchEvent', {
          type: 'touchStart',
          touchPoints: [{ ...point, id: 1 }],
        });
      } else {
        await page.mouse.move(point.x, point.y);
        await page.mouse.down();
      }
      pressed = true;
    },
    async move(point: Point) {
      if (touch !== undefined) {
        await touch.send('Input.dispatchTouchEvent', {
          type: 'touchMove',
          touchPoints: [{ ...point, id: 1 }],
        });
      } else await page.mouse.move(point.x, point.y);
    },
    async end() {
      if (!pressed) return;
      if (touch !== undefined)
        await touch.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
      else await page.mouse.up();
      pressed = false;
    },
    async close() {
      await touch?.detach();
    },
  };
}

for (const kind of ['mouse', 'touch'] as const) {
  test.describe(`375px kanban ${kind} drag`, () => {
    test.use({ viewport: { width: 375, height: 812 }, hasTouch: kind === 'touch' });

    test('auto-scrolls to another lane with a stationary pointer and drops in both directions', async ({
      signIn,
    }) => {
      const { page, api } = await signIn('test-user');
      const tag = unique();
      const ws = await api.createWorkspace(`칸반 가로 끌기 ${tag}`);
      const item = await api.createProjectItem(ws, tag);
      const title = `가장자리에서 다른 열로 끌기 ${tag}`;
      const task = str(await api.createTask(item, title), 'id');
      await page.goto(`/w/${ws}/i/${item}/tasks/board`);
      await expectViewport(page, true);
      const board = page.getByTestId('task-board');
      const card = board.locator(`[data-board-card="${task}"]`);
      await expect(card).toBeVisible();
      await board.scrollIntoViewIfNeeded();
      await page.evaluate(async () => document.fonts.ready);
      const input = await dragInput(page, kind);
      try {
        for (const right of [true, false]) {
          const bounds = await board.boundingBox();
          const handle = await card.locator('span[title]').boundingBox();
          if (bounds === null || handle === null) throw new Error('missing board or handle');
          const target = right ? '진행 중' : '할 일';
          const lane = board.locator(`[data-lane="${target}"]`);
          const point = { x: handle.x + handle.width / 2, y: handle.y + handle.height / 2 };
          await input.start(point);
          await input.move({
            x: right ? bounds.x + bounds.width - 12 : bounds.x + 12,
            y: point.y,
          });
          // 이 사이에는 포인터를 더 움직이지 않는다. pointermove 한 번에 몇 px만 옮기는 구현도 실패한다.
          await expect
            .poll(
              () =>
                lane.evaluate(
                  (el, { width, right }) => {
                    const board = el.closest<HTMLElement>('[data-testid="task-board"]');
                    if (board === null) throw new Error('missing board');
                    const scroll = board.scrollLeft;
                    return {
                      scrolled: right ? scroll > width * 0.65 : scroll < width * 0.35,
                      snap: getComputedStyle(board).scrollSnapType,
                      target: el.classList.contains('ring-2'),
                    };
                  },
                  { width: bounds.width, right },
                ),
              {
                intervals: [20],
                message:
                  'scroll position, snap and drop target update without another pointer move',
              },
            )
            .toEqual({ scrolled: true, snap: 'none', target: true });
          // 가운데로 옮겨 자동 스크롤을 멈춘 뒤 놓는다. 두 번째 열은 아직 일부만 보이는 상태다.
          await input.move({ x: bounds.x + bounds.width / 2, y: point.y });
          await input.end();
          await expect(lane.locator(`[data-task-card="${task}"]`)).toBeVisible();
          await expect
            .poll(async () => str(await api.get(`/api/tasks/${task}`), 'status'))
            .toBe(target);
          await expect(card.locator('[data-save-state]')).toHaveCount(0);
          await expect(board).toHaveCSS('scroll-snap-type', 'x mandatory');
          await expect
            .poll(() =>
              lane.evaluate((el) =>
                Math.abs(
                  el.getBoundingClientRect().left -
                    (el.parentElement?.getBoundingClientRect().left ?? 0),
                ),
              ),
            )
            .toBeLessThanOrEqual(1);
          expect(await pageOverflows(page)).toBe(false);
          await expect(page.getByRole('dialog')).toHaveCount(0);
        }
      } finally {
        await input.end();
        await input.close();
      }
      await page.reload();
      await expect(page.locator(`[data-lane="할 일"] [data-task-card="${task}"]`)).toBeVisible();
    });
  });
}
