import { expect, str, test, unique } from './fixtures';
import { expectViewport, pageOverflows } from './layout';
import { offenders } from './offenders';

for (const scale of [100, 200] as const) {
  test(`workspace limits save, validate and guard leaving at ${String(scale)}%`, async ({
    signIn,
  }, testInfo) => {
    const { page, api } = await signIn('test-user');
    const narrow = testInfo.project.name === 'narrow-375';
    const organization = await api.initialOrganization();
    const before = await api.get(`/api/organizations/${organization.id}`);
    if (
      typeof before !== 'object' ||
      before === null ||
      !('limits' in before) ||
      typeof before.limits !== 'object' ||
      before.limits === null
    )
      throw new Error('organization limits are missing');
    const previousRetention = {
      activity_retention_days:
        'activity_retention_days' in before.limits ? before.limits.activity_retention_days : null,
      audit_retention_days:
        'audit_retention_days' in before.limits ? before.limits.audit_retention_days : null,
    };
    const ws = await api.createWorkspace(
      `장기 공동연구와 자료 보존 공간 · Shared laboratory ${unique()}`,
    );
    try {
      await api.send('PUT', `/api/organizations/${organization.id}/settings`, {
        activity_retention_days: 90,
        audit_retention_days: 180,
      });
      await api.send('PUT', `/api/workspaces/${ws}/limits`, {
        max_file_bytes: 2 * 1024 ** 2,
        max_storage_bytes: null,
        max_concurrent_uploads: 3,
        activity_retention_days: 30,
        audit_retention_days: 60,
      });
      await page.goto(`/w/${ws}/members`);
      await expectViewport(page, narrow);
      if (scale === 200) {
        await page.addStyleTag({ content: 'html { font-size: 200%; }' });
        await page.evaluate(() => {
          document.documentElement.classList.add('dark');
        });
      }
      await page.evaluate(async () => {
        await document.fonts.ready;
      });
      await page.getByRole('tab', { name: '설정', exact: true }).click();
      await expect(page).toHaveURL(`/w/${ws}/settings`);
      await expect(page.getByTestId('member-list')).toHaveCount(0);
      const panel = page.getByRole('region', { name: '한도·보존' });
      const activity = panel.getByLabel('활동 보존 일수', { exact: true });
      await expect(activity).toHaveValue('30');
      await expect(
        panel.getByText(
          '파일 크기·저장량·동시 업로드 수에 실제로 쓰는 값은 서버 기본값·조직·공간 가운데 가장 작은 값입니다.',
        ),
      ).toBeVisible();
      await expect(
        panel.getByText(
          '보존 기간은 조직 값과 공간 값이 모두 있으면 두 값의 최솟값, 한쪽만 있으면 그 값을 씁니다. 둘 다 없으면 서버 설정값(따로 정하지 않았으면 활동 365일·감사 730일)을 씁니다.',
        ),
      ).toBeVisible();
      expect(await offenders(panel), 'limits stay within the panel').toEqual([]);
      const clippedSelections = await panel.locator('select').evaluateAll((selects) => {
        const canvas = document.createElement('canvas');
        const context = canvas.getContext('2d');
        if (context === null) throw new Error('text measurement is unavailable');
        return selects.flatMap((select) => {
          if (!(select instanceof HTMLSelectElement)) throw new Error('expected a select');
          const style = getComputedStyle(select);
          context.font = style.font;
          const text = select.selectedOptions[0]?.textContent ?? '';
          const available =
            select.clientWidth -
            Number.parseFloat(style.paddingLeft) -
            Number.parseFloat(style.paddingRight);
          return context.measureText(text).width > available + 1 ? [text] : [];
        });
      });
      expect(clippedSelections, 'selected values are readable in full').toEqual([]);
      expect(await pageOverflows(page), JSON.stringify(await offenders(page.locator('body')))).toBe(
        false,
      );

      await activity.fill('91');
      await expect(activity).toHaveAttribute('aria-invalid', 'true');
      await expect(panel.getByText('조직 값 이하여야 합니다.')).toBeVisible();
      await activity.fill('45');
      const changedRequest = page.waitForRequest(
        (request) =>
          request.method() === 'PUT' && request.url().endsWith(`/workspaces/${ws}/limits`),
      );
      await panel.getByRole('button', { name: '저장', exact: true }).click();
      expect((await changedRequest).postDataJSON()).toEqual({ activity_retention_days: 45 });
      await expect(panel.getByRole('button', { name: '저장', exact: true })).toBeDisabled();
      const current = await api.get(`/api/workspaces/${ws}/limits`);
      expect(current).toMatchObject({
        activity_retention_days: 45,
        audit_retention_days: 60,
        max_file_bytes: 2 * 1024 ** 2,
      });

      // Another administrator lowers the cap after this form loaded: the server 400 belongs to this field too.
      await api.send('PUT', `/api/organizations/${organization.id}/settings`, {
        activity_retention_days: 40,
      });
      await activity.fill('46');
      const rejected = page.waitForResponse(
        (response) =>
          response.request().method() === 'PUT' &&
          response.url().endsWith(`/workspaces/${ws}/limits`),
      );
      await panel.getByRole('button', { name: '저장', exact: true }).click();
      expect((await rejected).status()).toBe(400);
      await expect(activity).toHaveAttribute('aria-invalid', 'true');
      await expect(panel.getByText('조직 값 이하여야 합니다.')).toBeVisible();
      await expect(activity).toHaveValue('46');

      // Use the real sidebar navigation to ask about leaving, then verify the alert dialog's focus loop and return.
      if (narrow) await page.getByRole('button', { name: '사이드바 열고 닫기' }).click();
      const home = page.getByRole('link', { name: '홈', exact: true });
      await home.click();
      const confirm = page.getByRole('alertdialog', { name: '저장하지 않은 변경이 있습니다' });
      await expect(confirm).toBeVisible();
      await expect(confirm.getByText('닫으면 이 창에서 고친 내용이 사라집니다.')).toBeVisible();
      const focusInside = (): Promise<boolean> =>
        confirm.evaluate((el) => el.contains(document.activeElement));
      await expect.poll(focusInside).toBe(true);
      for (const key of ['Tab', 'Shift+Tab'] as const) {
        for (let i = 0; i < 6; i += 1) {
          await page.keyboard.press(key);
          expect(await focusInside()).toBe(true);
        }
      }
      expect(await offenders(confirm), 'confirmation content stays within the dialog').toEqual([]);
      await confirm.getByRole('button', { name: '계속 편집' }).click();
      await expect(confirm).toBeHidden();
      await expect(home).toBeFocused();
      if (narrow) {
        await page
          .getByRole('dialog', { name: '탐색 메뉴' })
          .getByRole('button', { name: '메뉴 닫기' })
          .click();
      }
      await expect(activity).toHaveValue('46');
      await panel.getByLabel('활동 보존 일수 설정').selectOption('unset');
      await panel.getByRole('button', { name: '저장', exact: true }).click();
      await expect(panel.getByRole('button', { name: '저장', exact: true })).toBeDisabled();
      expect(await api.get(`/api/workspaces/${ws}/limits`)).toMatchObject({
        activity_retention_days: null,
      });
      await page.screenshot({
        path: testInfo.outputPath(`workspace-limits-${String(scale)}.png`),
        fullPage: true,
      });
    } finally {
      await api.send('PUT', `/api/organizations/${organization.id}/settings`, previousRetention);
    }
    expect(await api.get(`/api/organizations/${organization.id}`)).toMatchObject({
      limits: previousRetention,
    });
  });
}

test('workspace tabs preserve administrator, member and readonly visibility', async ({
  signIn,
}) => {
  const owner = await signIn('test-user');
  const member = await signIn('test-user-2');
  const ws = await owner.api.createWorkspace(`권한별 한도 ${unique()}`);
  await owner.api.ensureOrganizationMember(member.api, member.user.email);
  await owner.api.invite(ws, member.user.email, 'member');
  await member.api.acceptInvitation(ws);
  const memberId = str(await member.api.get('/api/me'), 'id');
  for (const role of ['admin', 'member', 'readonly']) {
    await owner.api.send('PUT', `/api/workspaces/${ws}/members/${memberId}`, { role });
    await member.page.goto(`/w/${ws}/settings`);
    if (role === 'admin') {
      await expect(member.page.getByRole('tab')).toHaveCount(4);
      await expect(member.page.getByRole('region', { name: '한도·보존' })).toBeVisible();
      await expect(member.page.getByRole('link', { name: 'ZIP으로 내려받기' })).toBeVisible();
      await expect(member.page.getByRole('heading', { name: '위험 구역' })).toHaveCount(0);
    } else {
      await expect(member.page).toHaveURL(`/w/${ws}/members`);
      await expect(member.page.getByTestId('member-list')).toBeVisible();
      await expect(member.page.getByRole('tab')).toHaveText(['멤버', '그룹']);
      await expect(member.page.getByRole('region', { name: '한도·보존' })).toHaveCount(0);
      await member.page.goto(`/w/${ws}/invitations`);
      await expect(member.page).toHaveURL(`/w/${ws}/members`);
    }
    await member.page.getByRole('tab', { name: '그룹', exact: true }).click();
    await expect(member.page.getByRole('button', { name: '워크스페이스 탈퇴' })).toBeVisible();
    await expect(member.page.getByRole('textbox', { name: '새 그룹 이름' })).toHaveCount(
      role === 'admin' ? 1 : 0,
    );
  }
  await owner.page.goto(`/w/${ws}/settings`);
  await expect(owner.page.getByRole('region', { name: '한도·보존' })).toBeVisible();
  await expect(owner.page.getByRole('heading', { name: '위험 구역' })).toBeVisible();
});

for (const scale of [100, 200] as const) {
  test(`workspace management tabs, history and dialog focus at ${String(scale)}%`, async ({
    signIn,
  }, testInfo) => {
    const { page, api } = await signIn('test-user');
    const ws = await api.createWorkspace(`공간 관리 ${unique()}`);
    await page.goto(`/w/${ws}/members`);
    await expectViewport(page, testInfo.project.name === 'narrow-375');
    if (scale === 200) {
      await page.addStyleTag({ content: 'html { font-size: 200%; }' });
      await page.evaluate(() => {
        document.documentElement.classList.add('dark');
      });
    }
    await expect(page.getByTestId('member-list')).toBeVisible();
    const roleLimits = page.getByRole('button', { name: '역할별 공유 상한' });
    await expect(roleLimits).toHaveAttribute('aria-expanded', 'false');
    await roleLimits.click();
    await expect(page.getByText('공간 밖 조직원', { exact: true })).toBeVisible();
    await roleLimits.click();
    expect(await offenders(page.getByRole('tabpanel'))).toEqual([]);

    await page.getByRole('tab', { name: '초대', exact: true }).click();
    await expect(page).toHaveURL(`/w/${ws}/invitations`);
    await expect(page.getByTestId('member-list')).toHaveCount(0);
    const email = page.getByRole('textbox', { name: '이메일', exact: true });
    await email.fill('review@example.test');
    await page.getByRole('tab', { name: '그룹', exact: true }).click();
    const discard = page.getByRole('alertdialog');
    await expect(discard).toBeVisible();
    await discard.getByRole('button', { name: '계속 편집' }).click();
    await expect(page.getByRole('tab', { name: '그룹', exact: true })).toBeFocused();
    await expect(page.getByRole('tab', { name: '초대', exact: true })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    await expect(email).toHaveValue('review@example.test');
    await email.fill('');
    expect(await offenders(page.getByRole('tabpanel'))).toEqual([]);
    await page.goBack();
    await expect(page).toHaveURL(`/w/${ws}/members`);
    await expect(page.getByRole('tab', { name: '멤버', exact: true })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    await page.goForward();
    await expect(email).toBeVisible();

    await page.getByRole('tab', { name: '그룹', exact: true }).click();
    await expect(page).toHaveURL(`/w/${ws}/groups`);
    await page.getByRole('textbox', { name: '새 그룹 이름' }).fill('연구팀');
    await page.getByRole('button', { name: '그룹 만들기', exact: true }).click();
    const remove = page.getByRole('button', { name: '그룹 삭제', exact: true });
    await expect(remove).toBeVisible();
    expect(await offenders(page.getByRole('tabpanel'))).toEqual([]);
    await remove.click();
    const dialog = page.getByRole('dialog', { name: '‘연구팀’ 그룹을 삭제할까요?' });
    await expect(dialog).toBeVisible();
    await expect
      .poll(() => dialog.evaluate((el) => el.contains(document.activeElement)))
      .toBe(true);
    for (const key of ['Tab', 'Shift+Tab']) {
      for (let i = 0; i < 8; i += 1) {
        await page.keyboard.press(key);
        expect(await dialog.evaluate((el) => el.contains(document.activeElement))).toBe(true);
      }
    }
    expect(await offenders(dialog)).toEqual([]);
    await page.keyboard.press('Escape');
    await expect(remove).toBeFocused();

    await page.getByRole('tab', { name: '설정', exact: true }).click();
    await expect(page).toHaveURL(`/w/${ws}/settings`);
    await expect(page.getByRole('region', { name: '한도·보존' })).toBeVisible();
    await expect(page.getByRole('tabpanel').getByRole('heading')).toHaveText([
      '한도·보존',
      '데이터 반출',
      '위험 구역',
      '워크스페이스 삭제',
    ]);
    expect(await offenders(page.getByRole('tabpanel'))).toEqual([]);
    expect(await pageOverflows(page)).toBe(false);
    await page.screenshot({
      path: testInfo.outputPath(`workspace-tabs-${String(scale)}.png`),
      fullPage: true,
    });
  });
}
