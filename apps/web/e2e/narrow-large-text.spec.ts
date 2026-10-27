// 좁은 화면·어두움·200% 글꼴의 넘침(D-108 개정 2026-10-27, #150). Storybook은 화면마다 이 조건을 합친 스토리 하나만 두므로,
// 항목 묶음마다 따로 만들던 375px·200% 스토리가 눈으로만 보던 가로 넘침과 서랍을 실제 서버 화면에서 단언한다.
// 375px·500px viewport와 루트 글자 크기 200%이며 실제 휴대 기기·OS 글자 크기 시험이 아니다.
import type { Page } from '@playwright/test';

import { expect, test, unique } from './fixtures';
import { expectViewport, pageOverflows } from './layout';
import { offenders } from './offenders';

/** 루트 글자 크기를 200%로 키우고 서체를 다 읽은 뒤 잰다(Storybook의 글자 크기 도구 막대와 같은 방식). */
async function largeText(page: Page): Promise<void> {
  await page.addStyleTag({ content: 'html { font-size: 200%; }' });
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
}

test.describe('375px, dark, 200% text', () => {
  test.use({ viewport: { width: 375, height: 812 } });

  test('item group screens keep the group links and the page inside the viewport', async ({
    signIn,
  }) => {
    const { page, api } = await signIn('test-user');
    await api.send('PUT', '/api/me/settings', {
      theme: 'dark',
      locale: 'ko',
      time_zone: 'Asia/Seoul',
      week_start: 'mon',
      hour_cycle: 'h23',
    });
    const tag = unique();
    const ws = await api.createWorkspace(`좁은 큰 글꼴 ${tag}`);
    const item = await api.createProjectItem(ws, tag);
    await api.createTask(item, `큰 글꼴에서도 넘치지 않는 업무 제목 ${tag}`);
    const groups = page.getByRole('navigation', { name: '항목 묶음' });

    for (const [path, label] of [
      ['tasks/board', '업무'],
      ['docs', '문서'],
      ['events', '일정'],
      ['records', '기록'],
    ] as const) {
      await page.goto(`/w/${ws}/i/${item}/${path}`);
      await expectViewport(page, true);
      await expect(page.locator('html')).toHaveClass(/\bdark\b/);
      await largeText(page);
      await expect(groups.getByRole('link', { name: label })).toHaveAttribute(
        'aria-current',
        'page',
      );
      // 묶음 단추는 줄바꿈하며, 상자 안에서도 화면 폭 안에서도 넘치지 않는다.
      const nav = await groups.evaluate((el) => ({
        scroll: el.scrollWidth,
        client: el.clientWidth,
        right: Math.round(el.getBoundingClientRect().right),
      }));
      expect(nav.scroll, `${path} group links inner width`).toBeLessThanOrEqual(nav.client);
      expect(nav.right, `${path} group links inside the viewport`).toBeLessThanOrEqual(375);
      // 전체 가로 스크롤은 넘친 요소를 실패 메시지에 함께 보인다.
      const body = page.locator('body');
      expect(
        await pageOverflows(page),
        `${path} page scrolls horizontally; offenders=${JSON.stringify(await offenders(body))}`,
      ).toBe(false);
    }
  });
});

for (const width of [375, 500] as const) {
  test.describe(`${String(width)}px drawer`, () => {
    test.use({ viewport: { width, height: 800 } });

    // 768px 미만은 사이드바가 서랍이다. 머리글 단추로 열면 모듈 목록이 보이고 화면 폭 안에 든다.
    test('the navigation drawer opens with the module list inside the viewport', async ({
      signIn,
    }) => {
      const { page, api } = await signIn('test-user');
      const tag = unique();
      const ws = await api.createWorkspace(`서랍 ${tag}`);
      await api.createProjectItem(ws, tag);
      await page.goto(`/w/${ws}`);
      expect(await page.evaluate(() => window.innerWidth)).toBe(width);
      await page.getByRole('button', { name: '사이드바 열고 닫기' }).click();
      const drawer = page.getByRole('dialog', { name: '탐색 메뉴' });
      await expect(drawer).toBeVisible();
      const links = drawer.getByTestId('sidebar-modules').getByRole('link');
      await expect(links.first()).toBeVisible();
      const right = await drawer.evaluate((el) => Math.round(el.getBoundingClientRect().right));
      expect(right, 'drawer inside the viewport').toBeLessThanOrEqual(width);
      await drawer.getByRole('button', { name: '메뉴 닫기' }).click();
      await expect(drawer).toBeHidden();
      await expect(page.getByRole('button', { name: '사이드바 열고 닫기' })).toBeFocused();
      expect(await pageOverflows(page)).toBe(false);
    });
  });
}
