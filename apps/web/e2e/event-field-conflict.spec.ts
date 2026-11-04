import { expect, num, str, test, unique } from './fixtures';
import { pageOverflows } from './layout';

for (const difficult of [false, true]) {
  test.describe(difficult ? '375px dark 200%' : '1280px light', () => {
    test.use({ viewport: difficult ? { width: 375, height: 812 } : { width: 1280, height: 900 } });
    test('two tabs choose mixed fields, reopen on another 409 and save with the keyboard', async ({
      signIn,
    }) => {
      const { page, context, api } = await signIn('test-user');
      await api.send('PUT', '/api/me/settings', { theme: difficult ? 'dark' : 'light' });
      const tag = unique();
      const ws = await api.createWorkspace(`필드 비교 ${tag}`);
      const item = await api.createProjectItem(ws, tag);
      const event = str(
        await api.send('POST', `/api/items/${item}/events`, {
          request_id: crypto.randomUUID(),
          kind: 'meeting',
          title: `처음 ${tag}`,
          description: '처음 설명',
          all_day: true,
          start_date: '2026-11-02',
          end_date: '2026-11-02',
          starts_at: null,
          ends_at: null,
          timezone: 'Asia/Seoul',
        }),
        'id',
      );
      const path = `/w/${ws}/i/${item}/events`;
      const mineText = `${'내 설명의 긴 줄\n'.repeat(40)}마지막 줄`;
      const other = await context.newPage();
      await Promise.all([page.goto(path), other.goto(path)]);
      await page.locator(`[data-event-id="${event}"]`).click();
      await other.locator(`[data-event-id="${event}"]`).click();
      await expect(page.locator('#event-edit-title')).toBeVisible();
      await expect(other.locator('#event-edit-title')).toBeVisible();
      if (difficult) {
        await page.addStyleTag({ content: 'html { font-size: 200%; }' });
        await expect(page.locator('html')).toHaveClass(/\bdark\b/);
      }
      await page.locator('#event-edit-title').fill('내 제목');
      await page.locator('#event-edit-description').fill(mineText);
      await other.locator('#event-edit-title').fill('서버 제목');
      await other.locator('#event-edit-description').fill('서버 설명');
      await other.getByRole('button', { name: '저장', exact: true }).click();
      await expect(
        other.locator('[data-sonner-toast]').filter({ hasText: '저장했습니다.' }),
      ).toBeVisible();
      await expect.poll(async () => num(await api.get(`/api/events/${event}`), 'version')).toBe(2);
      await expect(
        other.getByRole('dialog').getByRole('heading', { name: '서버 제목', exact: true }),
      ).toBeFocused();
      expect(str(await api.get(`/api/events/${event}`), 'description')).toBe('서버 설명');

      const requests: { method: string; path: string }[] = [];
      page.on('request', (request) => {
        if (new URL(request.url()).pathname.startsWith('/api/'))
          requests.push({ method: request.method(), path: new URL(request.url()).pathname });
      });
      const before = requests.length;
      await page.getByRole('button', { name: '저장', exact: true }).click();
      const panel = page.getByRole('region', { name: '바뀐 내용 확인' });
      await expect(panel).toBeVisible();
      await expect(panel.getByRole('heading', { name: '바뀐 내용 확인' })).toBeFocused();
      expect(requests.slice(before)).toEqual([{ method: 'PUT', path: `/api/events/${event}` }]);
      await expect(panel.getByRole('radio', { name: '제목: 내 입력 사용' })).not.toBeChecked();
      await panel.locator('p').first().click();
      await page.keyboard.press('Escape');
      await expect(panel).toBeHidden();
      await expect(page.getByRole('dialog')).toBeVisible();
      await expect(page.locator('#event-edit-title')).toHaveValue('내 제목');
      await page.getByRole('button', { name: '저장', exact: true }).focus();
      await page.keyboard.press('Enter');
      await expect(panel).toBeVisible();
      await panel.getByRole('button', { name: '선택한 내용으로 저장' }).focus();
      await page.keyboard.press('Enter');
      await expect(panel.getByRole('radio', { name: '제목: 내 입력 사용' })).toBeFocused();
      await page.keyboard.press('ArrowRight');
      await expect(panel.getByRole('radio', { name: '제목: 서버 최신 사용' })).toBeChecked();
      await panel.getByRole('radio', { name: '설명: 내 입력 사용' }).focus();
      await page.keyboard.press('Space');

      await other.locator('#event-edit-description').fill('다시 바뀐 서버 설명');
      await other.getByRole('button', { name: '저장', exact: true }).click();
      await expect
        .poll(async () => str(await api.get(`/api/events/${event}`), 'description'))
        .toBe('다시 바뀐 서버 설명');
      await panel.getByRole('button', { name: '선택한 내용으로 저장' }).focus();
      await page.keyboard.press('Enter');
      await expect(panel.getByRole('textbox', { name: '설명: 서버 최신' })).toHaveValue(
        '다시 바뀐 서버 설명',
      );
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
        page.getByRole('dialog').getByRole('heading', { name: '서버 제목', exact: true }),
      ).toBeFocused();
      const stored = await api.get(`/api/events/${event}`);
      expect(str(stored, 'title')).toBe('서버 제목');
      expect(str(stored, 'description')).toBe(mineText);
      expect(num(stored, 'version')).toBe(4);
      await other.close();
    });
  });
}
