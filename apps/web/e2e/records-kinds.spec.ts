// 기록 종류 목록은 모든 모듈의 정의를 한 배열로 온다. 강의 종류의 `episode` 칸이 들어 있어도 프로젝트 항목의 기록 화면이
// 종류 단추를 그려야 한다(2단계 서버와 웹을 함께 배포했을 때의 실제 경로; CodeRabbit #140 591de1c3 지적).
import { expect, test, unique } from './fixtures';

test('a project item shows its record kinds while the server also serves course kinds with an episode field', async ({
  signIn,
}) => {
  const { page, api } = await signIn('test-user');
  const tag = unique();
  const ws = await api.createWorkspace(`기록 종류 ${tag}`);
  const item = await api.createProjectItem(ws, tag);
  // 이 서버가 실제로 episode 칸을 주는지 먼저 확인한다(없으면 이 시험은 아무것도 증명하지 않는다).
  const kinds = JSON.stringify(await api.get('/api/record-kinds'));
  expect(kinds).toContain('"type":"episode"');
  expect(kinds).toContain('"module_kind":"project"');

  await page.goto(`/w/${ws}/i/${item}`);
  await expect(page.getByRole('heading', { name: '기록', exact: true })).toBeVisible();
  await expect(page.getByRole('group', { name: '기록 종류' }).getByRole('button')).not.toHaveCount(
    0,
  );
  await expect(
    page.getByRole('alert').filter({ hasText: '기록을 불러오지 못했습니다.' }),
  ).toHaveCount(0);
});
