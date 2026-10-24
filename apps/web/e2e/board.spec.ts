// 칸반 이동·되돌리기의 서버 반영과, 키보드 초점·대화상자 취소·저장하지 않은 입력 보존의 대표 경로.
import type { Locator, Page } from '@playwright/test';

import { type Api, expect, str, test, unique } from './fixtures';

const lane = (page: Page, status: string) => page.locator(`section[aria-label="${status} 열"]`);
const card = (scope: Page | Locator, title: string) =>
  scope.locator('[data-task-card]').filter({ hasText: title });

async function serverStatus(api: Api, task: string): Promise<string> {
  return str(await api.get(`/api/tasks/${task}`), 'status');
}

async function openBoard(page: Page, ws: string, item: string): Promise<void> {
  await page.goto(`/w/${ws}/i/${item}`);
  await page.getByRole('button', { name: '칸반', exact: true }).click();
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
  await page.getByRole('menuitem', { name: '진행 중으로' }).click();
  await expect(card(lane(page, '진행 중'), title)).toBeVisible();
  await expect.poll(() => serverStatus(api, task)).toBe('in_progress');

  // 되돌리기.
  await page.getByRole('button', { name: `${title} 이동 되돌리기` }).click();
  await expect(card(lane(page, '할 일'), title)).toBeVisible();
  await expect.poll(() => serverStatus(api, task)).toBe('todo');

  // 키보드로 옮기기: 카드에 초점을 둔 채 Alt+오른쪽.
  await card(page, title).focus();
  await page.keyboard.press('Alt+ArrowRight');
  await expect(card(lane(page, '진행 중'), title)).toBeVisible();
  await expect.poll(() => serverStatus(api, task)).toBe('in_progress');
  await expect(card(page, title)).toBeFocused();

  // 새로 읽어도 서버의 자리 그대로다.
  await page.reload();
  await page.getByRole('button', { name: '칸반', exact: true }).click();
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
