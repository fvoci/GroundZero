// 간트는 표 안에서 가로로 스크롤되고 화면 전체는 넘치지 않는다(데스크톱 1280px·375px viewport).
import { expect, test, unique } from './fixtures';
import { expectViewport, pageOverflows } from './layout';

/** 서울 기준 오늘에서 며칠 뒤의 날짜(YYYY-MM-DD). */
function day(offset: number): string {
  const today = new Date(new Date().toLocaleString('en-US', { timeZone: 'Asia/Seoul' }));
  today.setDate(today.getDate() + offset);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${String(today.getFullYear())}-${pad(today.getMonth() + 1)}-${pad(today.getDate())}`;
}

test('gantt scrolls horizontally inside its own box', async ({ signIn }, testInfo) => {
  const { page, api } = await signIn('test-user');
  const tag = unique();
  const ws = await api.createWorkspace(`간트 시험 ${tag}`);
  const item = await api.createProjectItem(ws, tag);
  await api.createTask(item, `요구 사항 정리 ${tag}`, { start_date: day(0), due_date: day(6) });
  await api.createTask(item, `화면 설계와 검토 ${tag}`, { start_date: day(5), due_date: day(20) });
  await api.createTask(item, `배포와 회고 ${tag}`, { start_date: day(18), due_date: day(30) });

  await page.goto(`/w/${ws}/i/${item}`);
  const narrow = testInfo.project.name === 'narrow-375';
  await expectViewport(page, narrow);
  await page.getByRole('button', { name: '간트', exact: true }).click();
  const gantt = page.getByTestId('task-gantt');
  await expect(gantt.getByText(`요구 사항 정리 ${tag}`).first()).toBeVisible();

  const size = await gantt.evaluate((el) => ({ scroll: el.scrollWidth, client: el.clientWidth }));
  expect(await pageOverflows(page), 'page scrolls horizontally').toBe(false);
  if (narrow) {
    // 좁은 화면에서는 표가 상자보다 넓고, 상자 안에서만 움직인다.
    expect(size.scroll).toBeGreaterThan(size.client);
    await gantt.hover();
    await page.mouse.wheel(300, 0);
    await expect.poll(() => gantt.evaluate((el) => el.scrollLeft)).toBeGreaterThan(0);
    expect(await page.evaluate(() => window.scrollX)).toBe(0);
    expect(await pageOverflows(page)).toBe(false);
  }
});
