// 나3: 제목은 번호·아이콘 아래에서 카드 안쪽 폭을 쓰며, 좁은 본문은 열 단위로 넘긴다(D-108·D-110).
// 실제 Chromium viewport와 루트 글꼴 배율이며 실제 휴대 기기·OS 글꼴 설정 시험은 아니다.
import { expect, str, test, unique } from './fixtures';
import { expectViewport, measureWrap, pageOverflows } from './layout';
import { offenders } from './offenders';

const TITLES = [
  '좁은 화면에서도 제목을 넓게 읽으며 주간 회의 자료와 배포 점검표를 정리합니다',
  'https://example.test/workspaces/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/tasks/ffffffff-0000-1111-2222-333333333333',
];

for (const narrow of [false, true]) {
  test.describe(narrow ? '375px dark 200% kanban' : '1280px kanban', () => {
    test.use({ viewport: { width: narrow ? 375 : 1280, height: 812 } });

    test('gives card titles the full row and keeps lane scrolling inside the board', async ({
      signIn,
    }) => {
      const { page, api } = await signIn('test-user');
      await api.send('PUT', '/api/me/settings', {
        theme: narrow ? 'dark' : 'light',
        locale: 'ko',
        time_zone: 'Asia/Seoul',
        week_start: 'mon',
        hour_cycle: 'h23',
      });
      const tag = unique();
      const ws = await api.createWorkspace(`칸반 제목 폭 ${tag}`);
      const item = await api.createProjectItem(ws, tag);
      const tasks: string[] = [];
      for (const title of TITLES) tasks.push(str(await api.createTask(item, title), 'id'));
      await page.goto(`/w/${ws}/i/${item}/tasks/board`);
      await expectViewport(page, narrow);
      if (narrow) {
        await expect(page.locator('html')).toHaveClass(/\bdark\b/);
        await page.addStyleTag({ content: 'html { font-size: 200%; }' });
      }
      const board = page.getByTestId('task-board');
      await expect(board).toBeVisible();

      for (const task of tasks) {
        const card = board.locator(`[data-board-card="${task}"]`);
        const title = card.locator('[data-task-card] > span').last();
        await expect(title).toBeVisible();
        const wrap = await measureWrap(title);
        expect(wrap.overflow, 'long title stays inside its box').toBe(false);
        expect(await offenders(card), 'card contents stay inside the card').toEqual([]);
        const size = await title.evaluate((el) => {
          const card = el.closest('[data-board-card]');
          const menu = card?.querySelector('[aria-haspopup="menu"]');
          const timer = card?.querySelector('button[title]');
          if (
            card === null ||
            menu === null ||
            menu === undefined ||
            timer === null ||
            timer === undefined
          )
            throw new Error('missing card controls');
          const style = getComputedStyle(card);
          return {
            title: el.getBoundingClientRect().width,
            inside:
              card.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight),
            top: el.getBoundingClientRect().top,
            controlsBottom: Math.max(
              menu.getBoundingClientRect().bottom,
              timer.getBoundingClientRect().bottom,
            ),
          };
        });
        expect
          .soft(size.title / size.inside, 'title uses at least 80% of the card inside width')
          .toBeGreaterThanOrEqual(0.8);
        expect
          .soft(size.top, 'icons occupy the row above the title')
          .toBeGreaterThanOrEqual(size.controlsBottom);
      }

      expect(
        await pageOverflows(page),
        JSON.stringify(await offenders(page.locator('main').last())),
      ).toBe(false);
      const lanes = board.locator('[data-lane]');
      await expect(lanes).toHaveCount(3);
      if (narrow) {
        const size = await board.evaluate((el) => ({
          width: el.clientWidth,
          scroll: el.scrollWidth,
          lane: el.querySelector('[data-lane]')?.getBoundingClientRect().width ?? 0,
          snap: getComputedStyle(el).scrollSnapType,
        }));
        expect(size.lane / size.width, 'one lane fills the narrow body').toBeGreaterThanOrEqual(
          0.9,
        );
        expect(size.scroll, 'lanes scroll horizontally inside the board').toBeGreaterThan(
          size.width,
        );
        expect(size.snap).toBe('x mandatory');
        // 열 사이에서 손을 떼면 다음 열 시작으로 붙는다. CSS 속성만 확인하는 헛검사를 피한다.
        await board.evaluate((el) => {
          el.scrollLeft = el.clientWidth * 1.2;
        });
        await expect
          .poll(() =>
            lanes
              .nth(1)
              .evaluate((el) =>
                Math.abs(
                  el.getBoundingClientRect().left -
                    (el.parentElement?.getBoundingClientRect().left ?? 0),
                ),
              ),
          )
          .toBeLessThanOrEqual(1);
        await board.evaluate((el) => {
          el.scrollLeft = el.scrollWidth;
        });
        await expect
          .poll(() =>
            lanes
              .nth(2)
              .evaluate((el) =>
                Math.abs(
                  el.getBoundingClientRect().left -
                    (el.parentElement?.getBoundingClientRect().left ?? 0),
                ),
              ),
          )
          .toBeLessThanOrEqual(1);
      } else {
        const tops = await lanes.evaluateAll((els) =>
          els.map((el) => el.getBoundingClientRect().top),
        );
        expect(new Set(tops).size, 'wide body shows the three lanes side by side').toBe(1);
      }

      // 줄 배치를 바꿔도 Tab 순서와 Alt+→ 이동·초점을 유지한다. 좁은 화면은 옮긴 열까지 보여야 한다.
      const first = tasks[0];
      if (first === undefined) throw new Error('no task');
      const opener = board.locator(`[data-task-card="${first}"]`);
      const movedCard = board.locator(`[data-board-card="${first}"]`);
      await opener.focus();
      await page.keyboard.press('Tab');
      await expect(movedCard.locator('button[title]')).toBeFocused();
      await page.keyboard.press('Tab');
      await expect(movedCard.locator('[aria-haspopup="menu"]')).toBeFocused();
      await opener.focus();
      await page.keyboard.press('Alt+ArrowRight');
      await expect(lanes.nth(1).locator(`[data-task-card="${first}"]`)).toBeFocused();
      await expect
        .poll(async () => str(await api.get(`/api/tasks/${first}`), 'status'))
        .toBe('진행 중');
      await expect
        .poll(() =>
          movedCard.evaluate((el) => {
            const board = el.closest('[data-testid="task-board"]');
            if (board === null) throw new Error('no board');
            const box = el.getBoundingClientRect();
            const visible = board.getBoundingClientRect();
            return box.left >= visible.left - 1 && box.right <= visible.right + 1;
          }),
        )
        .toBe(true);
      expect(await pageOverflows(page)).toBe(false);
    });
  });
}
