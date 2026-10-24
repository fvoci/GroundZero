// 실제 테스트 OIDC(Keycloak) 로그인과 로그아웃 뒤의 접근 거부.
import { expect, test } from './fixtures';

test('logs in through the test OIDC provider and denies access after logout', async ({
  signIn,
}) => {
  const { page, api, endTrace } = await signIn('test-user');
  expect(await api.status('/api/me')).toBe(200);
  await page.goto('/notes');
  await expect(page.getByTestId('user-menu')).toBeVisible();

  // 로그아웃은 제공자 주소에 id_token_hint를 싣는다. 그 전에 trace를 끝낸다(실패하면 여기까지의 trace와 화면이 남는다).
  await endTrace();
  await page.getByTestId('user-menu').click();
  await page.getByRole('menuitem', { name: '로그아웃' }).click();

  // 앱의 로그인 화면으로 돌아오고, 같은 브라우저의 요청도 거절된다.
  await expect(page.getByRole('link', { name: '로그인' })).toBeVisible();
  expect(await api.status('/api/me')).toBe(401);
  expect(await api.status('/api/workspaces')).toBe(401);

  // 보호된 경로를 직접 열어도 로그인 화면이다.
  await page.goto('/notes');
  await expect(page.getByRole('link', { name: '로그인' })).toBeVisible();
  await expect(page.getByTestId('user-menu')).toHaveCount(0);

  // 제공자 세션도 끝나 다시 아이디·비밀번호를 묻는다(자동 재로그인 없음).
  await page.getByRole('link', { name: '로그인' }).click();
  await expect(page.locator('#username')).toBeVisible();
});
