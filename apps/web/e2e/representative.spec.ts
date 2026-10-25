// D-128 첫 대표 경로. HTTP mock·저장소 주입 없이 UI 조작 뒤 실제 서버 상태를 확인한다.
// 새 세션의 달력 시각만 하루 뒤로 고정한다. 서버 시간·실제 하루 경과·실기기 시험은 아니다.
import type { Locator, Page } from '@playwright/test';

import {
  expect,
  list,
  listField,
  nullableStr,
  selectOrganization,
  str,
  test,
  unique,
} from './fixtures';
import { expectViewport, pageOverflows } from './layout';

function day(offset: number): string {
  const today = new Intl.DateTimeFormat('en-CA', { timeZone: 'Asia/Seoul' }).format(new Date());
  const date = new Date(`${today}T00:00:00Z`);
  date.setUTCDate(date.getUTCDate() + offset);
  return date.toISOString().slice(0, 10);
}

/** 실제 Tab 왕복 후 focus-visible과 제품의 ring 그림자를 확인한다. 색 대비·시각 위계 검증은 별도다. */
async function keyboardFocus(target: Locator): Promise<void> {
  const page = target.page();
  await target.focus();
  await page.keyboard.press('Tab');
  const resting = await target.evaluate((el) => getComputedStyle(el).boxShadow);
  await page.keyboard.press('Shift+Tab');
  await expect(target).toBeFocused();
  expect(await target.evaluate((el) => el.matches(':focus-visible'))).toBe(true);
  // 투명 outline도 폭은 있을 수 있으므로 그것만으로 고리 PASS를 주지 않는다.
  await expect
    .poll(() => target.evaluate((el) => getComputedStyle(el).boxShadow))
    .not.toBe(resting);
  expect(await target.evaluate((el) => getComputedStyle(el).boxShadow)).not.toBe('none');
}

async function noOverflow(page: Page): Promise<void> {
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
  expect(await pageOverflows(page), 'scrollWidth <= clientWidth').toBe(false);
}

for (const theme of ['light', 'dark'] as const) {
  test(`D-128 ${theme}: create, assign, schedule, adjust and resume in a new session`, async ({
    signIn,
  }, testInfo) => {
    test.setTimeout(120_000);
    const first = await signIn('test-user');
    const { page, api } = first;
    // OS 색 모드나 CSS 강제 주입이 아니라 계정 설정을 저장한다(D-109).
    await api.send('PUT', '/api/me/settings', {
      theme,
      locale: 'ko',
      time_zone: 'Asia/Seoul',
      week_start: 'mon',
      hour_cycle: 'h23',
    });
    const organization = await api.initialOrganization();
    const me = await api.get('/api/me');
    const tag = unique();
    const ws = await api.createWorkspace(`대표 경로 ${tag}`);
    const item = await api.createProjectItem(ws, tag);
    const title = `출시 검토 ${tag}`;
    const predecessorTitle = `요구 사항 승인 ${tag}`;
    const predecessor = str(
      await api.createTask(item, predecessorTitle, {
        start_date: day(-2),
        due_date: day(-1),
      }),
      'id',
    );
    const boardPath = `/w/${ws}/i/${item}/tasks/board`;

    await test.step('항목에서 업무 만들기', async () => {
      await page.goto(boardPath);
      await expectViewport(page, testInfo.project.name === 'narrow-375');
      await expect(page.locator('html')).toHaveClass(
        theme === 'dark' ? /\bdark\b/ : /^(?!.*\bdark\b)/,
      );
      await expect(page.getByTestId('task-board')).toBeVisible();
      const input = page.getByRole('textbox', { name: '새 업무 제목', exact: true });
      await expect(input).toHaveAccessibleName('새 업무 제목');
      await input.fill(title);
      await input.press('Enter');
      await expect(page.locator('[data-task-card]').filter({ hasText: title })).toBeVisible();
      await noOverflow(page);
    });

    const task = list(await api.get(`/api/items/${item}/tasks`)).find(
      (t) => str(t, 'title') === title,
    );
    const taskId = str(task, 'id');
    const detailPath = `${boardPath}/t/${taskId}`;
    const card = page.locator(`[data-task-card="${taskId}"]`);
    await test.step('담당·기한·선행 지정', async () => {
      await expect(card).toHaveAccessibleName(new RegExp(title));
      await keyboardFocus(card);
      await page.keyboard.press('Enter');
      const dialog = page.getByRole('dialog', { name: title, exact: true });
      await expect(dialog).toBeVisible();
      await expect(page).toHaveURL(new RegExp(`${detailPath}$`));
      await dialog.getByLabel('시작일', { exact: true }).fill(day(0));
      await dialog.getByLabel('마감일', { exact: true }).fill(day(2));
      await dialog.getByRole('checkbox', { name: str(me, 'display_name'), exact: true }).check();
      await dialog.getByRole('button', { name: '저장', exact: true }).click();
      await expect
        .poll(async () => nullableStr(await api.get(`/api/tasks/${taskId}`), 'due_date'))
        .toBe(day(2));
      const saved = await api.get(`/api/tasks/${taskId}`);
      expect(listField(saved, 'assignees').map((a) => str(a, 'user_id'))).toContain(str(me, 'id'));
      await dialog.getByRole('button', { name: '선행 업무 추가', exact: true }).click();
      await page.getByRole('option', { name: new RegExp(predecessorTitle) }).click();
      await dialog.getByRole('button', { name: '관계 저장', exact: true }).click();
      await expect
        .poll(async () =>
          listField(await api.get(`/api/tasks/${taskId}`), 'predecessors').map((p) =>
            str(p, 'task_id'),
          ),
        )
        .toEqual([predecessor]);
      await noOverflow(page);
      await page.keyboard.press('Escape');
      await expect(dialog).toBeHidden();
      await expect(card).toBeFocused();
    });

    await test.step('칸반·간트에서 키보드로 조정', async () => {
      await keyboardFocus(card);
      await page.keyboard.press('Alt+ArrowRight');
      await expect
        .poll(async () => str(await api.get(`/api/tasks/${taskId}`), 'status_category'))
        .toBe('doing');
      await expect(card).toBeFocused();
      const ganttButton = page.getByRole('button', { name: '간트', exact: true });
      await keyboardFocus(ganttButton);
      await page.keyboard.press('Enter');
      await expect(page).toHaveURL(new RegExp(`/w/${ws}/i/${item}/tasks/gantt$`));
      const row = page.getByTestId('task-gantt').locator(`[data-task-card="${taskId}"]`);
      await expect(row).toHaveAccessibleName(new RegExp(title));
      await keyboardFocus(row);
      await page.keyboard.press('Alt+ArrowRight');
      // 밀릴 후속이 없으므로 불필요한 승인 대화 없이 저장한다(D-125).
      await expect
        .poll(async () => str(await api.get(`/api/tasks/${taskId}`), 'start_date'))
        .toBe(day(1));
      expect(str(await api.get(`/api/tasks/${taskId}`), 'due_date')).toBe(day(3));
      await expect(row).toBeFocused();
      await noOverflow(page);
    });

    await test.step('다음날 새 문맥의 내 업무·홈에서 찾아 이어서 하기', async () => {
      const next = await signIn('test-user');
      expect(next.context).not.toBe(first.context);
      await next.page.clock.setFixedTime(new Date(Date.now() + 86_400_000));
      await next.page.goto('/tasks');
      await selectOrganization(next.page, organization);
      await expect(next.page.locator('html')).toHaveClass(
        theme === 'dark' ? /\bdark\b/ : /^(?!.*\bdark\b)/,
      );
      const mine = next.page.getByRole('main').getByRole('link').filter({ hasText: title });
      await expect(mine).toHaveAttribute('href', detailPath);
      await keyboardFocus(mine);
      await noOverflow(next.page);
      await next.page.keyboard.press('Enter');
      const dialog = next.page.getByRole('dialog', { name: title, exact: true });
      await expect(dialog).toBeVisible();
      await expect(dialog.getByLabel('마감일', { exact: true })).toHaveValue(day(3));
      const continued = `다음날 검토를 이어갑니다 ${tag}`;
      await dialog.getByLabel('설명', { exact: true }).fill(continued);
      await dialog.getByRole('button', { name: '저장', exact: true }).click();
      await expect
        .poll(async () => str(await next.api.get(`/api/tasks/${taskId}`), 'description'))
        .toBe(continued);
      await next.page.keyboard.press('Escape');
      await expect(dialog).toBeHidden();
      await next.page.goto('/');
      const homeTask = next.page
        .getByRole('main')
        .getByRole('button')
        .filter({ hasText: title })
        .first();
      await expect(homeTask).toHaveAccessibleName(new RegExp(title));
      await keyboardFocus(homeTask);
      await noOverflow(next.page);
      await next.page.keyboard.press('Enter');
      await expect(next.page).toHaveURL(new RegExp(`${detailPath}$`));
      await expect(dialog.getByLabel('설명', { exact: true })).toHaveValue(continued);
    });
  });
}
