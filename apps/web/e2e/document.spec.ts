// 문서 만들기·한국어 입력·저장·새로고침·다시 열기. 실제 서버에 저장된 내용을 API로도 확인한다.
// 입력은 키보드 문자열 입력이며 OS 입력기(IME) 조합 시험이 아니다.
import { expect, str, test, unique } from './fixtures';

test('creates a wiki document, saves Korean text and keeps it after reload and reopen', async ({
  signIn,
}) => {
  const { page, api } = await signIn('test-user');
  const tag = unique();
  const ws = await api.createWorkspace(`문서 시험 ${tag}`);
  const title = `회의록 ${tag}`;
  const renamed = `주간 회의록 — 결정 사항 ${tag}`;
  const text = `다음 주 배포 일정을 확정했습니다. 담당자는 각자 점검 목록을 정리합니다 ${tag}`;

  await page.goto(`/w/${ws}`);
  await page.getByLabel('새 문서 제목').fill(title);
  await page.getByRole('button', { name: '문서 만들기' }).click();
  await page.waitForURL(new RegExp(`/w/${ws}/wiki/[0-9a-f-]{36}$`));
  const doc = page.url().split('/').at(-1) ?? '';

  await page.getByRole('textbox', { name: '제목', exact: true }).fill(renamed);
  await page.getByRole('textbox', { name: '본문' }).click();
  await page.keyboard.type(text);
  await page.getByRole('button', { name: '저장', exact: true }).click();
  await expect(page.getByTestId('save-status')).toHaveText('저장됨');

  // 서버에 저장된 값(자동 저장과 겹쳐도 마지막 내용이 서버에 있어야 한다).
  await expect
    .poll(async () => JSON.stringify(await api.get(`/api/documents/${doc}`)))
    .toContain(text);
  expect(str(await api.get(`/api/documents/${doc}`), 'title')).toBe(renamed);

  // 새로고침.
  await page.reload();
  await expect(page.getByRole('textbox', { name: '제목', exact: true })).toHaveValue(renamed);
  await expect(page.getByRole('textbox', { name: '본문' })).toContainText(text);

  // 워크스페이스 목록에서 다시 열기.
  await page.goto(`/w/${ws}`);
  await page.getByRole('main').getByRole('link', { name: renamed }).click();
  await expect(page).toHaveURL(new RegExp(`/wiki/${doc}$`));
  await expect(page.getByRole('textbox', { name: '본문' })).toContainText(text);
  await expect(page.getByTestId('save-status')).toHaveText('저장됨');
});
