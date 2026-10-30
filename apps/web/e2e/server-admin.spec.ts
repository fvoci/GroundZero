import { expect, str, test } from './fixtures';
import { expectViewport, measureWrap, pageOverflows } from './layout';
import { offenders } from './offenders';

for (const scale of [100, 200] as const) {
  test(`server management permissions, tabs and create dialog at font scale ${String(scale)}%`, async ({
    signIn,
  }, testInfo) => {
    const { page, api } = await signIn('test-user');
    const ordinary = await signIn('test-user-2');
    const me = await api.get('/api/me');
    expect(me).toHaveProperty('is_server_admin', true);
    expect(await ordinary.api.get('/api/me')).toHaveProperty('is_server_admin', false);
    await page.goto('/admin');
    await expect(page).toHaveURL('/admin/organizations');
    await expectViewport(page, testInfo.project.name === 'narrow-375');
    if (scale === 200) {
      await page.addStyleTag({ content: 'html { font-size: 200%; }' });
      await page.evaluate(() => {
        document.documentElement.classList.add('dark');
      });
    }
    // The entry lives in the personal menu, including the mobile sidebar drawer.
    if (testInfo.project.name === 'narrow-375')
      await page.getByRole('button', { name: '사이드바 열고 닫기' }).click();
    await page.getByTestId('user-menu').click();
    await page.getByRole('menuitem', { name: '서버 관리', exact: true }).click();
    const list = page.getByRole('list', { name: '조직', exact: true });
    await expect(list).toContainText('멤버');
    await expect(list).toContainText('공간');
    await expect(list).toContainText('만든 날');
    await expect(list.getByRole('button')).toHaveCount(0);
    await expect(list.getByRole('link')).toHaveCount(0);
    expect(await offenders(page.getByRole('tabpanel'))).toEqual([]);
    const tabs = page.getByTestId('management-tabs-scroll');
    const tabBounds = await tabs.evaluate((el) => {
      el.scrollLeft = el.scrollWidth;
      const last = el.querySelectorAll('[role=tab]')[1];
      if (last === undefined) throw new Error('missing server administrators tab');
      const tab = last.getBoundingClientRect();
      const box = el.getBoundingClientRect();
      return {
        overflow: getComputedStyle(el).overflowX,
        left: tab.left - box.left,
        right: tab.right - box.right,
      };
    });
    expect(tabBounds.overflow).toBe('auto');
    expect(tabBounds.left).toBeGreaterThanOrEqual(-1);
    expect(tabBounds.right).toBeLessThanOrEqual(1);
    const create = page
      .getByTestId('management-screen')
      .getByRole('button', { name: '조직 만들기', exact: true });
    await create.click();
    const dialog = page.getByRole('dialog', { name: '조직 만들기', exact: true });
    await expect(dialog).toBeVisible();
    const recipient = dialog.getByRole('combobox', { name: '첫 관리자', exact: true });
    await expect(recipient).toBeEnabled();
    await expect(recipient).toHaveValue(str(me, 'id'));
    const help = '첫 관리자는 조직에서 다른 사람을 초대하고 관리자 역할을 넘길 수 있습니다.';
    await expect(recipient).toHaveAccessibleDescription(help);
    expect((await measureWrap(dialog.getByText(help, { exact: true }))).overflow).toBe(false);
    await expect
      .poll(() => dialog.evaluate((el) => el.contains(document.activeElement)))
      .toBe(true);
    for (const key of ['Tab', 'Shift+Tab'])
      for (let i = 0; i < 8; i += 1) {
        await page.keyboard.press(key);
        expect(await dialog.evaluate((el) => el.contains(document.activeElement))).toBe(true);
      }
    expect(await offenders(dialog)).toEqual([]);
    const box = await dialog.evaluate((el) => {
      const r = el.getBoundingClientRect();
      return {
        left: r.left,
        right: r.right,
        top: r.top,
        bottom: r.bottom,
        width: window.innerWidth,
        height: window.innerHeight,
        scrolls: el.scrollHeight > el.clientHeight,
        overflowY: getComputedStyle(el).overflowY,
      };
    });
    expect(box.left).toBeGreaterThanOrEqual(0);
    expect(box.right).toBeLessThanOrEqual(box.width);
    expect(box.top).toBeGreaterThanOrEqual(0);
    expect(box.bottom).toBeLessThanOrEqual(box.height);
    if (box.scrolls) expect(box.overflowY).toBe('auto');
    const pageY = await page.evaluate(() => window.scrollY);
    await dialog.evaluate((el) => {
      el.scrollTop = el.scrollHeight;
    });
    expect(await page.evaluate(() => window.scrollY)).toBe(pageY);
    await page.keyboard.press('Escape');
    await expect(dialog).toHaveCount(0);
    await expect(create).toBeFocused();
    await create.click();
    await dialog
      .getByRole('textbox', { name: '이름', exact: true })
      .fill('긴 이름을 쓰는 공동 연구실과 여러 팀이 함께 관리할 조직');
    await page.keyboard.press('Escape');
    const discard = page.getByRole('alertdialog');
    await expect(discard).toBeVisible();
    await discard.getByRole('button', { name: '계속 편집', exact: true }).click();
    await expect(dialog.getByRole('textbox', { name: '이름', exact: true })).toHaveValue(
      '긴 이름을 쓰는 공동 연구실과 여러 팀이 함께 관리할 조직',
    );
    await page.keyboard.press('Escape');
    await discard.getByRole('button', { name: '변경 버리고 닫기', exact: true }).click();
    await expect(discard).toHaveCount(0);
    await expect(dialog).toHaveCount(0);
    await expect(create).toBeFocused();
    await page.getByRole('tab', { name: '서버 관리자', exact: true }).click();
    await expect(page).toHaveURL('/admin/admins');
    const admins = page.getByRole('list', { name: '서버 관리자', exact: true });
    await expect(admins).toContainText('지정일');
    await expect(admins.getByRole('button')).toHaveCount(0);
    expect(await offenders(page.getByRole('tabpanel'))).toEqual([]);
    await page.goBack();
    await expect(page.getByRole('tab', { name: '조직', exact: true })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    expect(await pageOverflows(page)).toBe(false);

    await ordinary.page.goto('/admin/admins');
    await expect(
      ordinary.page.getByText('이 조직을 볼 수 없습니다', { exact: true }),
    ).toBeVisible();
    await expect(ordinary.page.getByRole('link', { name: '홈으로 가기' })).toBeVisible();
    await expect(ordinary.page.getByRole('tab')).toHaveCount(0);
    expect(await ordinary.api.status('/api/server/admins')).toBe(403);
    if (testInfo.project.name === 'narrow-375')
      await ordinary.page.getByRole('button', { name: '사이드바 열고 닫기' }).click();
    await ordinary.page.getByTestId('user-menu').click();
    await expect(
      ordinary.page.getByRole('menuitem', { name: '서버 관리', exact: true }),
    ).toHaveCount(0);
  });
}
