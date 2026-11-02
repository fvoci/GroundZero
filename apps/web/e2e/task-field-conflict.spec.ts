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
      const task = str(
        await api.createTask(item, `처음 ${tag}`, { description: '처음 설명' }),
        'id',
      );
      const path = `/w/${ws}/i/${item}/tasks/board/t/${task}`;
      const mineText = `${'내 설명의 긴 줄\n'.repeat(40)}마지막 줄`;
      const other = await context.newPage();
      await Promise.all([page.goto(path), other.goto(path)]);
      await expect(page.locator('#task-title')).toBeVisible();
      await expect(other.locator('#task-title')).toBeVisible();
      if (difficult) {
        await page.addStyleTag({ content: 'html { font-size: 200%; }' });
        await expect(page.locator('html')).toHaveClass(/\bdark\b/);
      }
      await page.locator('#task-title').fill('내 제목');
      await page.locator('#task-description').fill(mineText);
      await other.locator('#task-title').fill('서버 제목');
      await other.locator('#task-description').fill('서버 설명');
      await other.locator('#task-save').click();
      await expect(
        other.locator('[data-sonner-toast]').filter({ hasText: '저장했습니다.' }),
      ).toBeVisible();

      const requests: { method: string; path: string }[] = [];
      page.on('request', (request) => {
        if (new URL(request.url()).pathname.startsWith('/api/'))
          requests.push({ method: request.method(), path: new URL(request.url()).pathname });
      });
      const before = requests.length;
      await page.locator('#task-save').click();
      const panel = page.getByRole('region', { name: '바뀐 내용 확인' });
      await expect(panel).toBeVisible();
      await expect(panel.getByRole('heading', { name: '바뀐 내용 확인' })).toBeFocused();
      expect(requests.slice(before)).toEqual([{ method: 'PATCH', path: `/api/tasks/${task}` }]);
      await expect(panel.getByRole('radio', { name: '제목: 내 입력 사용' })).not.toBeChecked();
      await panel.locator('p').first().click();
      await page.keyboard.press('Escape');
      await expect(panel).toBeHidden();
      await expect(page.getByRole('dialog')).toBeVisible();
      await expect(page.locator('#task-title')).toHaveValue('내 제목');
      await page.locator('#task-save').focus();
      await page.keyboard.press('Enter');
      await expect(panel).toBeVisible();
      await panel.getByRole('button', { name: '선택한 내용으로 저장' }).focus();
      await page.keyboard.press('Enter');
      await expect(panel.getByRole('radio', { name: '제목: 내 입력 사용' })).toBeFocused();
      await page.keyboard.press('ArrowRight');
      await expect(panel.getByRole('radio', { name: '제목: 서버 최신 사용' })).toBeChecked();
      await panel.getByRole('radio', { name: '설명: 내 입력 사용' }).focus();
      await page.keyboard.press('Space');

      await other.locator('#task-description').fill('다시 바뀐 서버 설명');
      await other.locator('#task-save').click();
      await expect
        .poll(async () => str(await api.get(`/api/tasks/${task}`), 'description'))
        .toBe('다시 바뀐 서버 설명');
      await panel.getByRole('button', { name: '선택한 내용으로 저장' }).focus();
      await page.keyboard.press('Enter');
      await expect(panel.getByRole('textbox', { name: '설명: 서버 최신' })).toHaveValue(
        '다시 바뀐 서버 설명',
      );
      await expect(panel.getByRole('heading', { name: '바뀐 내용 확인' })).toBeFocused();
      await expect(panel.getByRole('radio', { name: '설명: 내 입력 사용' })).not.toBeChecked();
      expect(requests.filter((r) => r.method === 'PATCH')).toHaveLength(3);
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
      await expect(page.locator('#task-save')).toBeFocused();
      const stored = await api.get(`/api/tasks/${task}`);
      expect(str(stored, 'title')).toBe('서버 제목');
      expect(str(stored, 'description')).toBe(mineText);
      expect(num(stored, 'version')).toBe(4);
      await other.close();
    });
  });
}
