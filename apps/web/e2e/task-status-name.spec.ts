// 항목이 정의한 상태 이름(D-116)은 이 단계의 화면이 기본 세 열로만 그리지만, 다른 칸만 고쳐 저장해도 서버의 상태 이름은
// 그대로여야 한다(CodeRabbit #139 df376f7c 지적). 2단계부터 상태 정의 API가 있어 실제 서버로 확인한다.
import { expect, num, str, test, unique } from './fixtures';

// 카드는 보드 전체에서 찾는다(이 단계는 상태별 열).
test('saving a title keeps an item-defined status name', async ({ signIn }) => {
  const { page, api } = await signIn('test-user');
  const tag = unique();
  const ws = await api.createWorkspace(`상태 이름 ${tag}`);
  const item = await api.createProjectItem(ws, tag);
  const status = await api.send('POST', `/api/items/${item}/statuses`, {
    name: 'review',
    category: 'doing',
    color: '',
  });
  expect(str(status, 'name')).toBe('review');
  const title = `검토 중인 업무 ${tag}`;
  const task = str(await api.createTask(item, title, { status: 'review' }), 'id');
  expect(str(await api.get(`/api/tasks/${task}`), 'status')).toBe('review');

  await page.goto(`/w/${ws}/i/${item}/tasks/board`);
  await expect(page.getByTestId('task-board')).toBeVisible();
  const card = page
    .getByTestId('task-board')
    .locator('[data-task-card]')
    .filter({ hasText: title });
  await expect(card).toBeVisible();
  await card.focus();
  await page.keyboard.press('Enter');
  const dialog = page.getByRole('dialog', { name: title });
  await expect(dialog).toBeVisible();
  // 상태 칸은 서버가 준 이름을 그대로 보인다.
  await expect(dialog.locator('[aria-labelledby="task-status-label"]')).toHaveText('review');

  await dialog.getByLabel('제목', { exact: true }).fill(`${title} (제목만 고침)`);
  await dialog.getByRole('button', { name: '저장', exact: true }).click();
  // 일반 완료는 토스트다(D-82).
  await expect(
    page.locator('[data-sonner-toast]').filter({ hasText: '저장했습니다.' }),
  ).toBeVisible();
  const saved = await api.get(`/api/tasks/${task}`);
  expect(str(saved, 'title')).toBe(`${title} (제목만 고침)`);
  expect(str(saved, 'status')).toBe('review');
  expect(num(saved, 'version')).toBe(2);
});
