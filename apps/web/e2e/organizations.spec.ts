import { expect, str, test, testUser, unique } from './fixtures';
import { expectViewport, measureWrap, pageOverflows } from './layout';
import { offenders } from './offenders';

// Real HTTP and isolated OIDC/DB. Both viewport projects also run the combined dark/200% condition.
for (const scale of [100, 200] as const) {
  test(`organization tabs, member privacy and join dialog at font scale ${String(scale)}%`, async ({
    signIn,
  }, testInfo) => {
    const { page, api } = await signIn('test-user');
    const member = await signIn('test-user-2');
    await api.ensureOrganizationMember(member.api, testUser('test-user-2').email);
    const organization = await api.initialOrganization();
    const tag = unique();
    const name = `긴 이름을 쓰는 공동 연구 공간과 여러 팀이 함께 보는 문서와 기록 ${tag}`;
    const workspace = await member.api.createWorkspace(name);

    await page.goto(`/o/${organization.id}`);
    await expect(page).toHaveURL(`/o/${organization.id}/members`);
    await expectViewport(page, testInfo.project.name === 'narrow-375');
    if (scale === 200) {
      await page.addStyleTag({ content: 'html { font-size: 200%; }' });
      await page.evaluate(() => {
        document.documentElement.classList.add('dark');
      });
    }
    await expect(page.getByRole('tab')).toHaveCount(4);
    await expect(page.getByRole('list', { name: '멤버' })).toBeVisible();
    const memberId = str(await member.api.get('/api/me'), 'id');
    const memberMenu = page.locator(`#org-member-menu-${memberId}`);
    await memberMenu.click();
    await page.getByRole('menuitem', { name: '내보내기', exact: true }).click();
    const removal = page.getByRole('dialog');
    await expect(removal).not.toContainText('불러오는 중');
    await expect
      .poll(() => removal.evaluate((el) => el.contains(document.activeElement)))
      .toBe(true);
    for (const key of ['Tab', 'Shift+Tab']) {
      for (let i = 0; i < 8; i += 1) {
        await page.keyboard.press(key);
        expect(await removal.evaluate((el) => el.contains(document.activeElement))).toBe(true);
      }
    }
    await page.keyboard.press('Escape');
    await expect(memberMenu).toBeFocused();
    const tabs = page.getByTestId('management-tabs-scroll');
    if (testInfo.project.name === 'narrow-375' && scale === 200)
      expect(
        await tabs.evaluate((el) => el.scrollWidth > el.clientWidth),
        'tabs scroll at 375px and 200%',
      ).toBe(true);
    await page.getByRole('tab', { name: '초대', exact: true }).click();
    await expect(page).toHaveURL(`/o/${organization.id}/invitations`);
    await page.goBack();
    await expect(page.getByRole('tab', { name: '멤버', exact: true })).toHaveAttribute(
      'aria-selected',
      'true',
    );

    await page.getByRole('tab', { name: '초대', exact: true }).click();
    const email = `long.organization.invitation.address.${tag}@organization-management.example.test`;
    await page.getByRole('textbox', { name: '이메일', exact: true }).fill(email);
    await page.getByRole('button', { name: '초대 보내기', exact: true }).click();
    await expect(
      page.getByRole('list', { name: '초대' }).getByText(email, { exact: true }),
    ).toBeVisible();
    expect(
      (
        await measureWrap(
          page.getByRole('list', { name: '초대' }).getByText(email, { exact: true }),
        )
      ).overflow,
    ).toBe(false);
    expect(await offenders(page.getByRole('tabpanel'))).toEqual([]);
    await page.getByRole('button', { name: `${email} 초대 철회`, exact: true }).click();
    await expect(
      page.getByRole('list', { name: '초대' }).getByText(email, { exact: true }),
    ).toHaveCount(0);

    await page.getByRole('tab', { name: '공간', exact: true }).click();
    const row = page.locator(`[data-organization-workspace="${workspace}"]`);
    const join = row.getByRole('button', { name: '관리자로 참여', exact: true });
    await join.click();
    const dialog = page.getByRole('dialog', {
      name: `${name} 공간에 관리자로 참여할까요?`,
      exact: true,
    });
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText('이 공간의 모든 문서와 기록을 볼 수 있게 됩니다.');
    await expect(dialog).toContainText('참여한 사실은 감사로그에 남습니다.');
    await expect(dialog).toContainText(
      '조직 관리자에서 내려가면 이 역할은 참여 전으로 돌아갑니다.',
    );
    await expect
      .poll(() => dialog.evaluate((el) => el.contains(document.activeElement)))
      .toBe(true);
    for (const key of ['Tab', 'Shift+Tab'])
      for (let i = 0; i < 8; i += 1) {
        await page.keyboard.press(key);
        expect(await dialog.evaluate((el) => el.contains(document.activeElement))).toBe(true);
      }
    expect(await offenders(dialog)).toEqual([]);
    await page.keyboard.press('Escape');
    await expect(join).toBeFocused();
    await join.click();
    await dialog.getByRole('button', { name: '관리자로 참여', exact: true }).click();
    await expect(dialog).toHaveCount(0);
    await expect(row.getByRole('link', { name: '열기', exact: true })).toBeFocused();
    expect(await offenders(page.getByRole('tabpanel'))).toEqual([]);

    await page.getByRole('tab', { name: '설정', exact: true }).click();
    const links = page.getByRole('switch', { name: '공개 링크 허용', exact: true });
    await links.click();
    const confirmation = page.getByRole('dialog', { name: '공개 링크 끄기', exact: true });
    await expect(confirmation).toContainText(
      '끄면 이 조직의 모든 공개 링크가 바로 열리지 않습니다.',
    );
    await page.keyboard.press('Escape');
    await expect(links).toBeFocused();
    await expect(links).toBeChecked();
    expect(await offenders(page.getByRole('tabpanel'))).toEqual([]);
    expect(await pageOverflows(page)).toBe(false);

    // An ordinary member deep linking to a management tab returns to the read-only members tab.
    await member.page.goto(`/o/${organization.id}/settings`);
    await expect(member.page).toHaveURL(`/o/${organization.id}/members`);
    await expect(member.page.getByRole('tab')).toHaveCount(1);
    await expect(member.page.getByRole('button', { name: / 동작$/ })).toHaveCount(0);
    await expect(member.page.getByRole('list', { name: '멤버' })).not.toContainText(
      testUser('test-user').email,
    );
  });
}
