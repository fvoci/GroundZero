// 한국어 줄바꿈과 가로 넘침(#118). 데스크톱 1280px와 375px viewport에서 돈다(실제 휴대 기기 시험이 아니다).
// 입력은 키보드 문자열 입력이며 OS 입력기(IME) 조합 시험이 아니다.
import { expect, test, unique } from './fixtures';
import { expectKoreanWrap, expectViewport, measureWrap, pageOverflows } from './layout';

const narrow = (name: string): boolean => name === 'narrow-375';

test('login screen keeps Korean words whole', async ({ anonymous }, testInfo) => {
  const page = await anonymous();
  await page.goto('/');
  await expectViewport(page, narrow(testInfo.project.name));
  await expectKoreanWrap(
    page.getByText('작업·지식·학습을 한곳에서 관리합니다.'),
    narrow(testInfo.project.name) ? 2 : 1,
  );
});

test('wiki settings description keeps Korean words whole', async ({ signIn }, testInfo) => {
  const { page, api } = await signIn('test-user');
  const ws = await api.createWorkspace(`줄바꿈 시험 ${unique()}`);
  await page.goto(`/w/${ws}`);
  await expectViewport(page, narrow(testInfo.project.name));
  await expectKoreanWrap(
    page.getByText('켜면 이 워크스페이스 문서의 본문을 바로 함께 고치고'),
    narrow(testInfo.project.name) ? 3 : 1,
  );
});

test('document share dialog keeps Korean words whole', async ({ signIn }, testInfo) => {
  const { page, api } = await signIn('test-user');
  const ws = await api.createWorkspace(`줄바꿈 시험 ${unique()}`);
  const doc = await api.createWikiDocument(ws, `공유 설명 문서 ${unique()}`);
  await page.goto(`/w/${ws}/wiki/${doc}`);
  await page.getByRole('button', { name: '공유·설정' }).click();
  const dialog = page.getByRole('dialog');
  await expectKoreanWrap(
    dialog.getByText('공유받은 사람은 이 원본을 읽거나 고칩니다.'),
    narrow(testInfo.project.name) ? 2 : 1,
  );
});

test('long Korean titles and unbroken strings wrap without overflow', async ({
  signIn,
}, testInfo) => {
  const { page, api } = await signIn('test-user');
  const tag = unique();
  const ws = await api.createWorkspace(`줄바꿈 시험 ${tag}`);
  const item = await api.createProjectItem(ws, tag);
  const longTitle = `좁은 화면에서도 긴 한국어 업무 제목이 어절 단위로만 줄을 바꾸는지 확인하는 합성 업무 ${tag}`;
  const url = `https://example.test/${'a1b2c3d4'.repeat(12)}?q=${tag}`;
  const solid = '띄어쓰기없이아주길게이어지는한국어문자열'.repeat(4);
  for (const title of [longTitle, url, solid]) await api.createTask(item, title);

  await page.goto(`/w/${ws}/i/${item}`);
  await page.getByRole('button', { name: '칸반', exact: true }).click();
  const card = (title: string) => page.locator('[data-task-card]').filter({ hasText: title });
  const titleWrap = await expectKoreanWrap(card(longTitle), narrow(testInfo.project.name) ? 2 : 1);
  expect(titleWrap.unavoidable).toEqual([]);

  // 공백 없는 문자열은 overflow-wrap: anywhere로 끊어도 된다. 넘치지만 않으면 된다.
  for (const title of [url, solid]) {
    const wrap = await measureWrap(card(title));
    expect(wrap.overflow, `${title.slice(0, 12)}… overflows`).toBe(false);
    expect(wrap.lines).toBeGreaterThan(1);
    expect(wrap.broken).toEqual([]);
  }
  expect(await pageOverflows(page)).toBe(false);
});

test('document body wraps unbroken text without overflow', async ({ signIn }, testInfo) => {
  const { page, api } = await signIn('test-user');
  const ws = await api.createWorkspace(`줄바꿈 시험 ${unique()}`);
  const doc = await api.createWikiDocument(ws, `본문 줄바꿈 ${unique()}`);
  await page.goto(`/w/${ws}/wiki/${doc}`);
  const body = page.getByRole('textbox', { name: '본문' });
  await body.click();
  await page.keyboard.insertText(
    '문서 본문의 일반 문장은 어절을 지키면서 줄을 바꿉니다. ' +
      `https://example.test/${'x9y8z7'.repeat(20)} ` +
      '공백없는한국어문자열'.repeat(8),
  );
  const wrap = await measureWrap(body);
  expect(wrap.broken).toEqual([]);
  expect(wrap.overflow).toBe(false);
  expect(wrap.lines).toBeGreaterThan(narrow(testInfo.project.name) ? 4 : 1);
  expect(await pageOverflows(page)).toBe(false);
});
