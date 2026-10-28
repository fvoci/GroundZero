// 칸반 이동·되돌리기의 서버 반영과, 키보드 초점·대화상자 취소·저장하지 않은 입력 보존의 대표 경로.
import type { Locator, Page } from '@playwright/test';

import { type Api, expect, str, test, unique } from './fixtures';

// 열은 항목의 상태 정의 이름이다(D-116). 새 항목은 서버가 심는 기본 세 상태 `할 일`·`진행 중`·`완료`(범주 todo·doing·done)로 시작한다.
const lane = (page: Page, status: string) => page.locator(`section[aria-label="${status} 열"]`);
const card = (scope: Page | Locator, title: string) =>
  scope.locator('[data-task-card]').filter({ hasText: title });

async function serverStatus(api: Api, task: string): Promise<string> {
  return str(await api.get(`/api/tasks/${task}`), 'status');
}

async function openBoard(page: Page, ws: string, item: string): Promise<void> {
  await page.goto(`/w/${ws}/i/${item}/tasks/board`);
  await expect(page.getByTestId('task-board')).toBeVisible();
}

test('moves a kanban card, undoes the move, and both reach the server', async ({ signIn }) => {
  const { page, api } = await signIn('test-user');
  const tag = unique();
  const ws = await api.createWorkspace(`칸반 시험 ${tag}`);
  const item = await api.createProjectItem(ws, tag);
  const title = `배포 점검표 정리 ${tag}`;
  const task = str(await api.createTask(item, title), 'id');
  await openBoard(page, ws, item);
  await expect(card(lane(page, '할 일'), title)).toBeVisible();

  // 메뉴로 옮기기.
  await page.getByRole('button', { name: `${title} 옮기기` }).click();
  await page.getByRole('menuitem', { name: '진행 중(으)로 옮기기' }).click();
  await expect(card(lane(page, '진행 중'), title)).toBeVisible();
  await expect.poll(() => serverStatus(api, task)).toBe('진행 중');

  // 되돌리기: 이동 완료 토스트의 동작 단추다(D-82).
  await page
    .locator('[data-sonner-toast]')
    .filter({ hasText: title })
    .getByRole('button', { name: '되돌리기' })
    .click();
  await expect(card(lane(page, '할 일'), title)).toBeVisible();
  await expect.poll(() => serverStatus(api, task)).toBe('할 일');

  // 서버 반영 뒤에도 화면의 앞선 이동 저장이 끝나야 다음 이동을 받는다.
  await expect(
    page
      .locator('[data-board-card]')
      .filter({ has: card(page, title) })
      .getByRole('button', { name: '→', exact: true, includeHidden: true }),
  ).toBeEnabled();

  // 키보드로 옮기기: 카드에 초점을 둔 채 Alt+오른쪽.
  await card(page, title).focus();
  await page.keyboard.press('Alt+ArrowRight');
  await expect(card(lane(page, '진행 중'), title)).toBeVisible();
  await expect.poll(() => serverStatus(api, task)).toBe('진행 중');
  await expect(card(page, title)).toBeFocused();

  // 새로 읽어도 서버의 자리 그대로다.
  await page.reload();
  await expect(card(lane(page, '진행 중'), title)).toBeVisible();
  await expect(card(lane(page, '할 일'), title)).toHaveCount(0);
});

test('keeps unsaved task input when closing is cancelled, and returns focus', async ({
  signIn,
}) => {
  const { page, api } = await signIn('test-user');
  const tag = unique();
  const ws = await api.createWorkspace(`대화상자 시험 ${tag}`);
  const item = await api.createProjectItem(ws, tag);
  const title = `회의 준비 ${tag}`;
  const task = str(await api.createTask(item, title), 'id');
  await openBoard(page, ws, item);

  // 키보드로 연다.
  await card(page, title).focus();
  await page.keyboard.press('Enter');
  const dialog = page.getByRole('dialog', { name: title });
  await expect(dialog).toBeVisible();
  const description = dialog.getByLabel('설명');
  const typed = `아직 저장하지 않은 메모입니다 ${tag}`;
  await description.fill(typed);

  // Esc: 닫지 않고 버릴지 묻는다. 다시 Esc(계속 편집): 입력은 그대로, 초점은 입력란으로 돌아간다.
  await page.keyboard.press('Escape');
  const confirm = page.getByRole('alertdialog', { name: '저장하지 않은 변경이 있습니다' });
  await expect(confirm).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(confirm).toBeHidden();
  await expect(dialog).toBeVisible();
  await expect(description).toHaveValue(typed);
  await expect(description).toBeFocused();

  // "계속 편집" 단추도 같다.
  await page.keyboard.press('Escape');
  await confirm.getByRole('button', { name: '계속 편집' }).click();
  await expect(description).toHaveValue(typed);

  // 버리고 닫으면 대화상자가 닫히고 초점은 카드로 돌아가며, 서버에는 아무것도 저장되지 않았다.
  await page.keyboard.press('Escape');
  await confirm.getByRole('button', { name: '변경 버리고 닫기' }).click();
  await expect(dialog).toBeHidden();
  await expect(card(page, title)).toBeFocused();
  expect(JSON.stringify(await api.get(`/api/tasks/${task}`))).not.toContain(typed);
});

test('keeps new task and view input when details open, close, or discard only detail changes', async ({
  signIn,
}) => {
  const { page, api } = await signIn('test-user');
  const tag = unique();
  const ws = await api.createWorkspace(`보드 입력 보존 ${tag}`);
  const item = await api.createProjectItem(ws, tag);
  const title = `기존 업무 ${tag}`;
  const task = str(await api.createTask(item, title), 'id');
  await openBoard(page, ws, item);
  const newTitle = `만들지 않은 업무 ${tag}`;
  const viewName = `저장하지 않은 보기 ${tag}`;
  const newTask = page.getByRole('textbox', { name: '새 업무 제목', exact: true });
  const view = page.getByLabel('보기 이름', { exact: true });
  await newTask.fill(newTitle);
  await page.getByTestId('task-filter-toggle').click();
  await view.fill(viewName);
  const dialog = page.getByRole('dialog', { name: title, exact: true });

  await card(page, title).click();
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: '닫기', exact: true }).click();
  await expect(dialog).toBeHidden();
  await expect(newTask).toHaveValue(newTitle);
  await expect(view).toHaveValue(viewName);

  await card(page, title).click();
  await dialog.getByLabel('설명', { exact: true }).fill(`버릴 상세 입력 ${tag}`);
  await page.keyboard.press('Escape');
  const confirm = page.getByRole('alertdialog', { name: '저장하지 않은 변경이 있습니다' });
  await confirm.getByRole('button', { name: '변경 버리고 닫기' }).click();
  await expect(dialog).toBeHidden();
  await expect(card(page, title)).toBeFocused();
  await expect(newTask).toHaveValue(newTitle);
  await expect(view).toHaveValue(viewName);
  expect(str(await api.get(`/api/tasks/${task}`), 'description')).toBe('');
  expect(JSON.stringify(await api.get(`/api/items/${item}/tasks`))).not.toContain(newTitle);
});
