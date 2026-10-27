// 공통 대화상자(`$lib/ui/dialog`)의 초점 가두기, 긴 제목과 닫기 단추의 겹침, 내부 스크롤(D-108 개정 2026-10-27, #150).
// Storybook은 부품마다 기본 스토리 하나만 두므로, 걷어낸 Dialog 스토리(KeyboardSave의 Tab 가두기·LongContentScroll)가 맡던
// 단언을 같은 클래스(`max-h-[90vh] overflow-y-auto`)를 쓰는 실제 업무 상세 대화상자에서 한다. 1280px·375px viewport, 루트 글자 크기
// 100%·200%이며 실제 휴대 기기·OS 글자 크기 시험이 아니다. 초점 복귀·Escape는 board.spec.ts·representative.spec.ts가 본다.
import { expect, str, test, unique } from './fixtures';
import { expectViewport, pageOverflows } from './layout';

const LONG_TITLE =
  '좁은 화면에서도 대화상자 제목이 오른쪽 위 닫기 단추 밑으로 들어가지 않는지 확인하는 아주 긴 한국어 업무 제목';

for (const scale of [100, 200] as const) {
  test(`a dialog with a long title keeps focus inside and scrolls inside at font scale ${String(scale)}%`, async ({
    signIn,
  }, testInfo) => {
    const { page, api } = await signIn('test-user');
    const narrow = testInfo.project.name === 'narrow-375';
    const tag = unique();
    const ws = await api.createWorkspace(`대화상자 배치 ${tag}`);
    const item = await api.createProjectItem(ws, tag);
    const title = `${LONG_TITLE} ${tag}`;
    const task = str(await api.createTask(item, title), 'id');

    await page.goto(`/w/${ws}/i/${item}/tasks/board`);
    await expectViewport(page, narrow);
    if (scale !== 100)
      await page.addStyleTag({ content: `html { font-size: ${String(scale)}%; }` });
    await page.evaluate(async () => {
      await document.fonts.ready;
    });
    await page.locator(`[data-task-card="${task}"]`).click();
    const dialog = page.getByRole('dialog', { name: title, exact: true });
    await expect(dialog).toBeVisible();
    const pageY = await page.evaluate(() => window.scrollY);

    const focusInside = (): Promise<boolean> =>
      dialog.evaluate((el) => el.contains(document.activeElement));
    // 열면 초점이 대화상자 안으로 들어오고, Tab·Shift+Tab은 밖으로 나가지 않는다.
    await expect.poll(focusInside).toBe(true);
    for (const key of ['Tab', 'Shift+Tab'] as const) {
      for (let i = 0; i < 12; i += 1) {
        await page.keyboard.press(key);
        expect(await focusInside(), `${key} #${String(i + 1)} stays inside the dialog`).toBe(true);
      }
    }

    // 긴 제목은 오른쪽 위 닫기 단추 밑으로 들어가지 않는다(글이 놓이는 곳은 오른쪽 안쪽 여백을 뺀 자리다).
    const overlap = await dialog.evaluate((el) => {
      const title = el.querySelector('[data-slot=dialog-title]');
      const close = el.querySelector('[data-slot=dialog-close]');
      if (title === null || close === null) return null;
      const t = title.getBoundingClientRect();
      const c = close.getBoundingClientRect();
      const textRight = t.right - Number.parseFloat(getComputedStyle(title).paddingRight);
      return c.left < textRight && c.top < t.bottom && c.bottom > t.top;
    });
    expect(overlap, 'title and close button are both drawn').not.toBeNull();
    expect(overlap, 'long title runs under the close button').toBe(false);

    // 대화상자는 화면 안에 들고, 길면 안에서 스크롤되며 바깥 페이지는 움직이지 않는다.
    const box = await dialog.evaluate((el) => {
      const r = el.getBoundingClientRect();
      return {
        left: r.left,
        right: r.right,
        top: r.top,
        bottom: r.bottom,
        overflowY: getComputedStyle(el).overflowY,
        scrolls: el.scrollHeight > el.clientHeight,
        width: window.innerWidth,
        height: window.innerHeight,
      };
    });
    expect(box.left).toBeGreaterThanOrEqual(0);
    expect(box.right).toBeLessThanOrEqual(box.width);
    expect(box.top).toBeGreaterThanOrEqual(0);
    expect(box.bottom).toBeLessThanOrEqual(box.height);
    if (box.scrolls) expect(['auto', 'scroll']).toContain(box.overflowY);
    // 글자 200%의 좁은 화면은 내용이 대화상자보다 길어 내부 스크롤이 생긴다(헛검사 방지).
    if (narrow && scale === 200) expect(box.scrolls, 'dialog content scrolls inside').toBe(true);
    await dialog.evaluate((el) => {
      el.scrollTop = el.scrollHeight;
    });
    expect(await page.evaluate(() => window.scrollY), 'page behind the dialog stays put').toBe(
      pageY,
    );
    expect(await pageOverflows(page)).toBe(false);
  });
}
