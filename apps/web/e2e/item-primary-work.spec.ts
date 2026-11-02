// 나2: 항목 머리의 보조 편집보다 묶음·업무를 먼저 보이고, 접기·메뉴에서도 입력과 초점을 지킨다(D-110).
import { expect, listField, str, test, unique } from './fixtures';
import { expectViewport, pageOverflows } from './layout';
import { offenders } from './offenders';

test('item primary work precedes collapsed information and auxiliary actions', async ({
  signIn,
}, testInfo) => {
  const { page, api } = await signIn('test-user');
  const narrow = testInfo.project.name === 'narrow-375';
  const tag = unique();
  const ws = await api.createWorkspace(`항목 배치 ${tag}`);
  const item = await api.createProjectItem(ws, tag);
  await api.createTask(item, `업무 ${tag}`);
  const profileResponse = page.waitForResponse(
    (response) =>
      response.request().method() === 'GET' &&
      response.url().endsWith(`/api/items/${item}/profile`),
  );
  await page.goto(`/w/${ws}/i/${item}/tasks/board`);
  const response = await profileResponse;
  expect(response.status()).toBe(200);
  const loadedProfile: unknown = await response.json();
  await expectViewport(page, narrow);
  if (narrow) await page.addStyleTag({ content: 'html { font-size: 200%; }' });
  await page.evaluate((dark) => document.documentElement.classList.toggle('dark', dark), narrow);
  const info = page.getByRole('button', { name: /^항목 정보/ });
  await expect(info).toHaveAttribute('aria-expanded', 'false');
  const summaryStatus = info.getByRole('status');
  await expect(summaryStatus).toContainText(
    `멤버 ${String(listField(loadedProfile, 'members').length)}명`,
  );
  await expect(summaryStatus).not.toContainText('불러오는 중…');
  await expect(summaryStatus).not.toContainText('항목 정보를 불러오지 못했습니다.');
  await expect(page.locator('#tasks-heading')).toBeVisible();
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
  const groups = page.getByRole('navigation', { name: '항목 묶음' });
  if (!narrow) {
    // 설계의 이전 기준 y≈690보다 분명히 위에 묶음과 업무 보드가 시작해야 한다.
    expect((await groups.boundingBox())?.y).toBeLessThan(450);
    expect((await page.locator('#tasks-heading').boundingBox())?.y).toBeLessThan(500);
  }
  // 칸반은 자체 가로 스크롤이 있으므로 머리의 보임·잘림과 페이지 넘침을 따로 확인한다.
  expect(await offenders(page.locator('[data-item-header]'))).toEqual([]);
  expect(await pageOverflows(page)).toBe(false);
  const csv = page.getByRole('button', { name: '여러 업무 한꺼번에 등록' });
  await expect(csv).toHaveAttribute('aria-expanded', 'false');
  await expect(page.getByLabel('새 업무 제목', { exact: true })).toBeVisible();
  await csv.click();
  await expect(csv).toHaveAttribute('aria-expanded', 'true');
  await csv.click();

  await info.click();
  await expect(info).toHaveAttribute('aria-expanded', 'true');
  const title = page.locator('#item-title');
  const previous = await title.inputValue();
  await title.fill(`편집 ${tag}`);
  await expect(info).toBeDisabled();
  await info.dispatchEvent('click');
  await expect(info).toHaveAttribute('aria-expanded', 'true');
  await expect(title).toHaveValue(`편집 ${tag}`);
  await page
    .getByRole('navigation', { name: '경로', exact: true })
    .getByRole('link')
    .first()
    .click();
  const discard = page.getByRole('alertdialog');
  await expect(discard).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(discard).not.toBeVisible();
  await expect(title).toHaveValue(`편집 ${tag}`);
  await title.fill(previous);
  await info.click();
  await expect(info).toHaveAttribute('aria-expanded', 'false');

  const more = page.getByRole('button', { name: '더 보기', exact: true });
  await more.click();
  await expect(page.getByRole('menuitem', { name: '보관', exact: true })).toBeVisible();
  await expect(page.getByRole('menuitem', { name: '템플릿으로 표시' })).toBeVisible();
  await page.getByRole('menuitem', { name: '항목 지우기' }).click();
  await expect(page.getByRole('button', { name: '휴지통으로', exact: true })).toBeFocused();
  await page.getByRole('button', { name: '취소', exact: true }).click();
  await expect(more).toBeFocused();
  await more.click();
  await page.getByRole('menuitem', { name: '복제', exact: true }).click();
  const copy = page.getByRole('dialog', { name: '항목 복제' });
  await expect(copy).toBeVisible();
  const focusInside = () => copy.evaluate((el) => el.contains(document.activeElement));
  await expect.poll(focusInside).toBe(true);
  for (const key of ['Tab', 'Shift+Tab']) {
    for (let i = 0; i < 8; i += 1) {
      await page.keyboard.press(key);
      expect(await focusInside()).toBe(true);
    }
  }
  expect(await offenders(copy)).toEqual([]);
  await page.keyboard.press('Escape');
  await expect(copy).not.toBeVisible();
  await expect(more).toBeFocused();
  await more.click();
  await page.getByRole('menuitem', { name: '복제', exact: true }).click();
  await copy.getByLabel('새 제목', { exact: true }).fill(`사본 편집 ${tag}`);
  await page.keyboard.press('Escape');
  await expect(discard).toBeVisible();
  await discard.getByRole('button', { name: '계속 편집' }).click();
  await expect(copy).toBeVisible();
  await expect(copy.getByLabel('새 제목', { exact: true })).toHaveValue(`사본 편집 ${tag}`);
  await page.keyboard.press('Escape');
  await discard.getByRole('button', { name: '변경 버리고 닫기' }).click();
  await expect(copy).not.toBeVisible();
  await expect(more).toBeFocused();
});

test('only entering a newly created empty item starts with information expanded', async ({
  signIn,
}) => {
  const { page, api } = await signIn('test-user');
  const tag = unique();
  const ws = await api.createWorkspace(`새 항목 ${tag}`);
  const existing = await api.createProjectItem(ws, tag);
  const mod = str(await api.get(`/api/items/${existing}`), 'module_id');
  await page.goto(`/w/${ws}/m/${mod}`);
  await page.locator('#new-item').fill(`빈 항목 ${tag}`);
  await page.getByRole('button', { name: '항목 만들기', exact: true }).click();
  const info = page.getByRole('button', { name: /^항목 정보/ });
  await expect(info).toHaveAttribute('aria-expanded', 'true');
  await page.reload();
  await expect(info).toHaveAttribute('aria-expanded', 'false');
});

// #291 D1: retain real HTTP authorization while holding only the module's dispatch.
test('document backlinks start alongside module validation and keep layout and keyboard focus', async ({
  signIn,
}, testInfo) => {
  const { page, api } = await signIn('test-user');
  const narrow = testInfo.project.name === 'narrow-375';
  const tag = unique();
  const ws = await api.createWorkspace(`문서 병렬 읽기 ${tag}`);
  const item = await api.createProjectItem(ws, tag);
  const module = str(await api.get(`/api/items/${item}`), 'module_id');
  const modulePath = `/api/modules/${module}`;
  const backlinksPath = `/api/items/${item}/backlinks`;
  let releaseModule!: () => void;
  const moduleGate = new Promise<void>((done) => {
    releaseModule = done;
  });
  await page.route(`**${modulePath}`, async (route) => {
    await moduleGate;
    await route.continue();
  });
  let backlinksRequests = 0;
  page.on('request', (request) => {
    if (new URL(request.url()).pathname === backlinksPath) backlinksRequests += 1;
  });
  const pendingBacklinks = page.waitForResponse(
    (response) => new URL(response.url()).pathname === backlinksPath,
  );
  try {
    await page.goto(`/w/${ws}/i/${item}/docs`);
    expect((await pendingBacklinks).status()).toBe(200);
    await expect(page.locator('#backlinks-heading')).toHaveCount(0);
  } finally {
    releaseModule();
  }
  await expect(page.locator('#item-docs-heading')).toBeVisible();
  await expect(page.locator('#backlinks-heading')).toBeVisible();
  await expectViewport(page, narrow);
  if (narrow) await page.addStyleTag({ content: 'html { font-size: 200%; }' });
  await page.evaluate((dark) => document.documentElement.classList.toggle('dark', dark), narrow);
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
  expect(await pageOverflows(page)).toBe(false);
  expect(await offenders(page.locator('section[aria-labelledby="item-docs-heading"]'))).toEqual([]);
  expect(await offenders(page.locator('section[aria-labelledby="backlinks-heading"]'))).toEqual([]);
  const groups = page.getByRole('navigation', { name: '항목 묶음' });
  await groups.getByRole('link', { name: '업무', exact: true }).focus();
  await page.keyboard.press('Tab');
  await expect(groups.getByRole('link', { name: '문서', exact: true })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(groups.getByRole('link', { name: '문서', exact: true })).toHaveAttribute(
    'aria-current',
    'page',
  );
  expect(backlinksRequests).toBe(1);
});
