import { expect, test, unique } from './fixtures';
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

test('only a workspace administrator sees limits and retention', async ({ signIn }) => {
  const owner = await signIn('test-user');
  const member = await signIn('test-user-2');
  const ws = await owner.api.createWorkspace(`권한별 한도 ${unique()}`);
  await owner.api.ensureOrganizationMember(member.api, member.user.email);
  await owner.api.invite(ws, member.user.email, 'member');
  await member.api.acceptInvitation(ws);
  await member.page.goto(`/w/${ws}/members`);
  await expect(member.page.getByTestId('member-list')).toBeVisible();
  await expect(member.page.getByRole('region', { name: '한도·보존' })).toHaveCount(0);
  await owner.page.goto(`/w/${ws}/members`);
  await expect(owner.page.getByRole('region', { name: '한도·보존' })).toBeVisible();
});
