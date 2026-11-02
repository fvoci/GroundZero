import type { Page } from '@playwright/test';
import { expect, num, str, test, unique } from './fixtures';
import { pageOverflows } from './layout';

// 항목 정보·프로필의 판 충돌(D-94 개정, D-104, 승인 6568773043): 실제 두 탭에서 첫 409에 바로 칸마다 고르고, 다시 바뀌면
// 바로 다시 고르며, 키보드만으로 고른다. 375px·어두움·200%에서 넘침·잘림·세로 배치를 본다. 409 뒤 후속 GET은 없다.

async function openInfo(page: Page): Promise<void> {
  const toggle = page.getByRole('button', { name: /^항목 정보/ });
  await expect(toggle).toBeVisible();
  if ((await toggle.getAttribute('aria-expanded')) !== 'true') await toggle.click();
  await expect(page.getByRole('form', { name: '항목 정보' })).toBeVisible();
}

function apiRequests(page: Page): { method: string; path: string }[] {
  const requests: { method: string; path: string }[] = [];
  page.on('request', (request) => {
    const path = new URL(request.url()).pathname;
    if (path.startsWith('/api/')) requests.push({ method: request.method(), path });
  });
  return requests;
}

for (const difficult of [false, true]) {
  test.describe(difficult ? '375px dark 200%' : '1280px light', () => {
    test.use({ viewport: difficult ? { width: 375, height: 812 } : { width: 1280, height: 900 } });

    test('two tabs choose mixed item fields, reopen on another 409 and save with the keyboard', async ({
      signIn,
    }) => {
      const { page, context, api } = await signIn('test-user');
      await api.send('PUT', '/api/me/settings', { theme: difficult ? 'dark' : 'light' });
      const tag = unique();
      const ws = await api.createWorkspace(`항목 비교 ${tag}`);
      const item = await api.createProjectItem(ws, tag);
      const path = `/w/${ws}/i/${item}/docs`;
      const start = num(await api.get(`/api/items/${item}`), 'version');
      const mineText = `${'내 설명의 긴 줄\n'.repeat(40)}마지막 줄`;
      const other = await context.newPage();
      await Promise.all([page.goto(path), other.goto(path)]);
      await openInfo(page);
      await openInfo(other);
      if (difficult) {
        await page.addStyleTag({ content: 'html { font-size: 200%; }' });
        await expect(page.locator('html')).toHaveClass(/\bdark\b/);
      }
      await page.locator('#item-title').fill('내 제목');
      await page.locator('#item-description').fill(mineText);
      await other.locator('#item-title').fill('서버 제목');
      await other.locator('#item-description').fill('서버 설명');
      await other
        .getByRole('form', { name: '항목 정보' })
        .getByRole('button', { name: '저장', exact: true })
        .click();
      await expect
        .poll(async () => str(await api.get(`/api/items/${item}`), 'title'))
        .toBe('서버 제목');

      const requests = apiRequests(page);
      const form = page.getByRole('form', { name: '항목 정보' });
      await form.getByRole('button', { name: '저장', exact: true }).click();
      const panel = page.getByRole('region', { name: '바뀐 내용 확인' });
      await expect(panel).toBeVisible();
      await expect(panel.getByRole('heading', { name: '바뀐 내용 확인' })).toBeFocused();
      expect(requests).toEqual([{ method: 'PUT', path: `/api/items/${item}` }]);
      await expect(panel.getByRole('radio', { name: '제목: 내 입력 사용' })).not.toBeChecked();
      // Escape는 비교만 닫고 내 입력을 그대로 둔다.
      await page.keyboard.press('Escape');
      await expect(panel).toBeHidden();
      await expect(page.locator('#item-title')).toHaveValue('내 제목');
      await form.getByRole('button', { name: '저장', exact: true }).focus();
      await page.keyboard.press('Enter');
      await expect(panel).toBeVisible();
      await panel.getByRole('button', { name: '선택한 내용으로 저장' }).focus();
      await page.keyboard.press('Enter');
      await expect(panel.getByRole('radio', { name: '제목: 내 입력 사용' })).toBeFocused();
      await page.keyboard.press('ArrowRight');
      await expect(panel.getByRole('radio', { name: '제목: 서버 최신 사용' })).toBeChecked();
      await panel.getByRole('radio', { name: '설명: 내 입력 사용' }).focus();
      await page.keyboard.press('Space');

      await other.locator('#item-description').fill('다시 바뀐 서버 설명');
      await other
        .getByRole('form', { name: '항목 정보' })
        .getByRole('button', { name: '저장', exact: true })
        .click();
      await expect
        .poll(async () => str(await api.get(`/api/items/${item}`), 'description'))
        .toBe('다시 바뀐 서버 설명');
      await panel.getByRole('button', { name: '선택한 내용으로 저장' }).focus();
      await page.keyboard.press('Enter');
      await expect(panel.getByRole('textbox', { name: '설명: 서버 최신' })).toHaveValue(
        '다시 바뀐 서버 설명',
      );
      await expect(panel.getByText('확인 중 다시 변경되었습니다', { exact: false })).toBeVisible();
      await expect(panel.getByRole('heading', { name: '바뀐 내용 확인' })).toBeFocused();
      await expect(panel.getByRole('radio', { name: '설명: 내 입력 사용' })).not.toBeChecked();
      expect(requests.filter((r) => r.method === 'PUT')).toHaveLength(3);
      expect(requests.filter((r) => r.method === 'GET')).toHaveLength(0);
      expect(await pageOverflows(page)).toBe(false);
      expect(
        await panel
          .locator('label, button')
          .evaluateAll((elements) =>
            elements.every((element) => element.scrollWidth <= element.clientWidth),
          ),
      ).toBe(true);
      const pair = panel.locator('[data-conflict-pair="description"]');
      const mineBox = await pair.getByRole('textbox', { name: '설명: 내 입력' }).boundingBox();
      const serverBox = await pair.getByRole('textbox', { name: '설명: 서버 최신' }).boundingBox();
      expect(mineBox).not.toBeNull();
      expect(serverBox).not.toBeNull();
      if (difficult && mineBox && serverBox)
        expect(serverBox.y).toBeGreaterThanOrEqual(mineBox.y + mineBox.height);
      await panel.getByRole('radio', { name: '설명: 내 입력 사용' }).focus();
      await page.keyboard.press('Space');
      await page.keyboard.press('Tab');
      await expect(panel.getByRole('textbox', { name: '설명: 내 입력' })).toBeFocused();
      await page.keyboard.press('Control+End');
      expect(
        await panel
          .getByRole('textbox', { name: '설명: 내 입력' })
          .evaluate((element) => element.scrollTop),
      ).toBeGreaterThan(0);
      await panel.getByRole('button', { name: '선택한 내용으로 저장' }).focus();
      await page.keyboard.press('Enter');
      await expect(panel).toBeHidden();
      await expect(
        page.locator('[data-sonner-toast]').filter({ hasText: '저장했습니다.' }),
      ).toBeVisible();
      const stored = await api.get(`/api/items/${item}`);
      expect(str(stored, 'title')).toBe('서버 제목');
      expect(str(stored, 'description')).toBe(mineText);
      expect(num(stored, 'version')).toBe(start + 3);
      await other.close();
    });

    test('two tabs choose profile fields, keep the server-only period and save on the reviewed version', async ({
      signIn,
    }) => {
      const { page, context, api } = await signIn('test-user');
      await api.send('PUT', '/api/me/settings', { theme: difficult ? 'dark' : 'light' });
      const tag = unique();
      const ws = await api.createWorkspace(`프로필 비교 ${tag}`);
      const item = await api.createProjectItem(ws, tag);
      const path = `/w/${ws}/i/${item}/docs`;
      const start = num(await api.get(`/api/items/${item}/profile`), 'version');
      const other = await context.newPage();
      await Promise.all([page.goto(path), other.goto(path)]);
      await openInfo(page);
      await openInfo(other);
      if (difficult) {
        await page.addStyleTag({ content: 'html { font-size: 200%; }' });
        await expect(page.locator('html')).toHaveClass(/\bdark\b/);
      }
      const profileOf = (tab: Page) =>
        tab.locator('section[aria-labelledby="item-profile-heading"]');
      await profileOf(page).getByRole('button', { name: '고치기' }).click();
      await profileOf(other).getByRole('button', { name: '고치기' }).click();
      await page.locator('#item-goal').fill('내 목표');
      await other.locator('#item-goal').fill('서버 목표');
      await other.locator('#item-starts').fill('2026-11-05');
      await profileOf(other).getByRole('button', { name: '저장', exact: true }).click();
      await expect
        .poll(async () => num(await api.get(`/api/items/${item}/profile`), 'version'))
        .toBe(start + 1);

      const requests = apiRequests(page);
      await profileOf(page).getByRole('button', { name: '저장', exact: true }).click();
      const panel = page.getByRole('region', { name: '바뀐 내용 확인' });
      await expect(panel).toBeVisible();
      await expect(panel.getByRole('heading', { name: '바뀐 내용 확인' })).toBeFocused();
      await expect(panel.getByText('다른 사람이 바꾼 칸 1개(서버 값 유지)')).toBeVisible();
      await panel.getByRole('radio', { name: '목표: 내 입력 사용' }).focus();
      await page.keyboard.press('Space');
      // 409 뒤 비교를 여는 데 후속 GET은 없다.
      expect(requests).toEqual([{ method: 'PUT', path: `/api/items/${item}/profile` }]);
      await panel.getByRole('button', { name: '선택한 내용으로 저장' }).focus();
      await page.keyboard.press('Enter');
      await expect(panel).toBeHidden();
      await expect
        .poll(async () => str(await api.get(`/api/items/${item}/profile`), 'goal'))
        .toBe('내 목표');
      const stored = await api.get(`/api/items/${item}/profile`);
      expect(str(stored, 'starts_on')).toBe('2026-11-05');
      expect(num(stored, 'version')).toBe(start + 2);
      // 선택 저장은 PUT 하나다(뒤의 GET은 일반 저장과 같은 성공 뒤 다시 읽기).
      expect(requests.filter((r) => r.method === 'PUT')).toHaveLength(2);
      expect(await pageOverflows(page)).toBe(false);
      await other.close();
    });
  });
}
