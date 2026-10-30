import { expect, list, str, test, testUser, unique } from './fixtures';
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
    const ownedWorkspace = await api.createWorkspace(`소유권 제안 검증 ${tag}`);
    const memberMe = await member.api.get('/api/me');
    const memberId = str(memberMe, 'id');
    const memberName = str(memberMe, 'display_name');
    await api.send('PUT', `/api/organizations/${organization.id}/members/${memberId}`, {
      role: 'admin',
    });
    // Existing members and readonly members can both join as administrator (SR-253-1).
    await member.api.invite(
      workspace,
      testUser('test-user').email,
      scale === 100 ? 'member' : 'readonly',
    );
    await api.acceptInvitation(workspace);

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
    await expect(page.getByRole('tab', { name: '멤버 2명', exact: true })).toBeVisible();
    await expect(page.getByRole('searchbox')).toHaveCount(0);
    const memberMenu = page.locator(`#org-member-menu-${memberId}`);
    await expect(memberMenu).toHaveAccessibleName(`${memberName} 동작`);
    await expect(memberMenu.locator('svg')).toBeVisible();
    await expect(memberMenu).toHaveText('');
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
    await memberMenu.click();
    await page.getByRole('menuitem', { name: '멤버로 바꾸기', exact: true }).click();
    const demotion = page.getByRole('dialog');
    await expect(demotion).toContainText(
      `${memberName}님은 조직 관리 탭을 더 볼 수 없습니다. '관리자로 참여'로 받은 공간 역할은 참여 전으로 돌아갑니다.`,
    );
    await demotion.getByRole('button', { name: '멤버로 바꾸기', exact: true }).click();
    await expect(demotion).toHaveCount(0);
    await expect(memberMenu).toBeFocused();
    const tabs = page.getByTestId('management-tabs-scroll');
    const overflows = await tabs.evaluate((el) => el.scrollWidth > el.clientWidth);
    if (testInfo.project.name === 'narrow-375' && scale === 200)
      expect(overflows, 'tabs scroll at 375px and 200%').toBe(true);
    if (overflows) {
      await tabs.evaluate((el) => {
        el.scrollLeft = 0;
      });
      const left = page.getByTestId('management-tabs-fade-left');
      const right = page.getByTestId('management-tabs-fade-right');
      await expect(right).toBeVisible();
      await expect(right).toHaveCSS('background-image', /linear-gradient/);
      await expect(left).toHaveCount(0);
      await tabs.evaluate((el) => {
        el.scrollLeft = el.scrollWidth;
      });
      await expect(left).toBeVisible();
      await expect(right).toHaveCount(0);
      await tabs.evaluate((el) => {
        el.scrollLeft = 0;
      });
      await expect(right).toBeVisible();
    } else {
      await expect(page.getByTestId('management-tabs-fade-left')).toHaveCount(0);
      await expect(page.getByTestId('management-tabs-fade-right')).toHaveCount(0);
    }
    await page.getByRole('tab', { name: '초대', exact: true }).click();
    await expect(page).toHaveURL(`/o/${organization.id}/invitations`);
    await page.goBack();
    await expect(page.getByRole('tab', { name: '멤버 2명', exact: true })).toHaveAttribute(
      'aria-selected',
      'true',
    );

    await page.getByRole('tab', { name: '초대', exact: true }).click();
    await page
      .getByRole('textbox', { name: '이메일', exact: true })
      .fill(testUser('test-user-2').email);
    await page.getByRole('button', { name: '초대 보내기', exact: true }).click();
    await expect(page.getByRole('alert')).toContainText([
      '이 이메일은 이미 이 조직의 멤버입니다. 초대할 수 없습니다.',
    ]);
    await expect(page.getByRole('textbox', { name: '이메일', exact: true })).toHaveValue(
      testUser('test-user-2').email,
    );
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
    // Another request closes the invitation first; the stale row's 404 must keep this screen.
    const invitation = list(
      await api.get(`/api/organizations/${organization.id}/invitations`),
    ).find((entry) => str(entry, 'email') === email);
    const closed = await page.request.delete(`/api/invitations/${str(invitation, 'id')}`, {
      headers: {
        origin: new URL(page.url()).origin,
        'x-fvoci-expected-user': str(await api.get('/api/me'), 'id'),
      },
    });
    expect(closed.status()).toBe(204);
    const pendingEmail = `next.${tag}@organization-management.example.test`;
    await page.getByRole('textbox', { name: '이메일', exact: true }).fill(pendingEmail);
    await page.getByRole('button', { name: `${email} 초대 철회`, exact: true }).click();
    await expect(
      page.getByRole('list', { name: '초대' }).getByText(email, { exact: true }),
    ).toHaveCount(0);
    await expect(page.getByRole('textbox', { name: '이메일', exact: true })).toHaveValue(
      pendingEmail,
    );
    await page.getByRole('textbox', { name: '이메일', exact: true }).fill('');

    await page.getByRole('tab', { name: '공간', exact: true }).click();
    const row = page.locator(`[data-organization-workspace="${workspace}"]`);
    await expect(row.getByRole('link', { name: '열기', exact: true })).toBeVisible();
    await expect(row).toContainText('멤버 2명');
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
    await expect(row).toContainText('멤버 2명');
    await expect(row.getByRole('button', { name: '관리자로 참여', exact: true })).toHaveCount(0);
    await row.getByRole('button', { name: '소유권 이전 제안', exact: true }).click();
    const ownership = page.getByRole('dialog', { name: '소유권 이전 제안', exact: true });
    const recipient = ownership.getByRole('combobox', { name: '받을 사람', exact: true });
    await expect(recipient).toBeEnabled();
    await expect(recipient).toHaveAccessibleDescription('이 공간의 멤버에게만 제안할 수 있습니다');
    // The two known organization members are the current owner and the current user.
    await expect(recipient.getByRole('option')).toHaveCount(1);
    await expect(ownership).toContainText('제안할 다른 조직 멤버가 없습니다');
    await page.keyboard.press('Escape');
    await expect(row.getByRole('button', { name: '소유권 이전 제안', exact: true })).toBeFocused();
    const ownedRow = page.locator(`[data-organization-workspace="${ownedWorkspace}"]`);
    const offer = ownedRow.getByRole('button', { name: '소유권 이전 제안', exact: true });
    await offer.click();
    const rejectedOffer = page.getByRole('dialog', { name: '소유권 이전 제안', exact: true });
    const rejectedRecipient = rejectedOffer.getByRole('combobox', {
      name: '받을 사람',
      exact: true,
    });
    await rejectedRecipient.selectOption(memberId);
    await rejectedOffer.getByRole('button', { name: '제안 보내기', exact: true }).click();
    await expect(rejectedOffer.getByRole('alert')).toContainText([
      '이 공간의 멤버에게만 제안할 수 있습니다',
    ]);
    await expect(rejectedRecipient).toHaveAttribute('aria-invalid', 'true');
    await expect(rejectedRecipient).toHaveAccessibleDescription(
      '이 공간의 멤버에게만 제안할 수 있습니다',
    );
    // The announcement stays available to assistive technology while only the description is drawn.
    await expect
      .poll(() =>
        rejectedOffer
          .getByText('이 공간의 멤버에게만 제안할 수 있습니다', { exact: true })
          .evaluateAll(
            (elements) =>
              elements.filter((element) => {
                const style = getComputedStyle(element);
                return (
                  style.display !== 'none' &&
                  style.visibility === 'visible' &&
                  style.getPropertyValue('clip') === 'auto' &&
                  style.clipPath === 'none'
                );
              }).length,
          ),
      )
      .toBe(1);
    expect(await offenders(rejectedOffer)).toEqual([]);
    await rejectedRecipient.selectOption('');
    await page.keyboard.press('Escape');
    await expect(offer).toBeFocused();
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
