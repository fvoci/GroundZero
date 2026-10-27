// 항목 묶음과 모듈 보기는 하위 경로다(D-121, 사용자 R7 A). 항목 뿌리 경로는 종류별 기본 묶음(프로젝트 업무 보드)으로
// 바뀌고(replaceState), 묶음 전환은 기록을 쌓아 뒤로 가기로 돌아온다. 모듈 달력은 달력 보기에서만 보인다.
import { expect, str, test, unique } from './fixtures';

test('an item opens on its default group, switches groups by path and goes back', async ({
  signIn,
}) => {
  const { page, api } = await signIn('test-user');
  const tag = unique();
  const ws = await api.createWorkspace(`묶음 ${tag}`);
  const item = await api.createProjectItem(ws, tag);
  const groups = page.getByRole('navigation', { name: '항목 묶음' });

  await page.goto(`/w/${ws}/i/${item}`);
  await expect(page).toHaveURL(new RegExp(`/w/${ws}/i/${item}/tasks/board$`));
  await expect(groups.getByRole('link', { name: '업무' })).toHaveAttribute('aria-current', 'page');
  await expect(page.getByRole('heading', { name: '업무', exact: true })).toBeVisible();
  await expect(page.getByLabel('새 문서 제목')).toHaveCount(0);

  await groups.getByRole('link', { name: '문서' }).click();
  await expect(page).toHaveURL(new RegExp(`/w/${ws}/i/${item}/docs$`));
  await expect(page.getByLabel('새 문서 제목')).toBeVisible();
  await expect(page.getByRole('heading', { name: '업무', exact: true })).toHaveCount(0);

  await page.goBack();
  await expect(page).toHaveURL(new RegExp(`/w/${ws}/i/${item}/tasks/board$`));
  await expect(page.getByRole('heading', { name: '업무', exact: true })).toBeVisible();

  // 같은 탭에서 뿌리 경로로 다시 열면 마지막 묶음을 기억한다.
  await groups.getByRole('link', { name: '일정' }).click();
  await expect(page).toHaveURL(new RegExp(`/w/${ws}/i/${item}/events$`));
  await page.goto(`/w/${ws}/i/${item}`);
  await expect(page).toHaveURL(new RegExp(`/w/${ws}/i/${item}/events$`));

  const mod = str(await api.get(`/api/items/${item}`), 'module_id');
  const views = page.getByRole('navigation', { name: '모듈 보기' });
  await page.goto(`/w/${ws}/m/${mod}`);
  await expect(views.getByRole('link', { name: '항목' })).toHaveAttribute('aria-current', 'page');
  await expect(page.getByTestId('item-list')).toBeVisible();
  await views.getByRole('link', { name: '달력' }).click();
  await expect(page).toHaveURL(new RegExp(`/w/${ws}/m/${mod}/calendar$`));
  await expect(page.getByTestId('item-list')).toHaveCount(0);
  await expect(page.locator('#calendar-heading')).toBeVisible();
});
