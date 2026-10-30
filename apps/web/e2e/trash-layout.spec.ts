// 나8: 휴지통 제목 폭·보존 칸 간격과 같은 줄 확인 흐름(D-108·D-110).
// Chromium viewport·루트 글꼴 배율이며 실제 휴대 기기·OS 글꼴 설정 시험은 아니다.
import { writeFileSync } from 'node:fs';

import type { Locator } from '@playwright/test';

import { expect, str, test, unique } from './fixtures';
import { expectViewport, measureWrap, pageOverflows } from './layout';
import { offenders } from './offenders';

async function rowSize(row: Locator) {
  return row.evaluate((el) => {
    const title = el.querySelector(':scope > span');
    if (title === null) throw new Error('missing trash title');
    const box = title.getBoundingClientRect();
    const style = getComputedStyle(el);
    return {
      title: box.width,
      inside: el.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight),
      titleTop: box.top,
      titleBottom: box.bottom,
      actions: [...el.querySelectorAll(':scope > :not(:first-child)')].map((action) => {
        const rect = action.getBoundingClientRect();
        return { top: rect.top, bottom: rect.bottom, left: rect.left };
      }),
    };
  });
}

for (const narrow of [false, true]) {
  test.describe(narrow ? '375px dark 200% trash' : '1280px trash', () => {
    test.use({ viewport: { width: narrow ? 375 : 1280, height: 812 } });

    test('gives titles room and preserves restore and purge confirmation', async ({
      signIn,
    }, testInfo) => {
      const { page, api } = await signIn('test-user');
      await api.send('PUT', '/api/me/settings', {
        theme: narrow ? 'dark' : 'light',
        locale: 'ko',
        time_zone: 'Asia/Seoul',
        week_start: 'mon',
        hour_cycle: 'h23',
      });
      const tag = unique();
      const ws = await api.createWorkspace(`휴지통 제목 폭 ${tag}`);
      const titles = [
        '좁은 화면에서도 긴 한국어 문서 제목을 넓게 읽으며 주간 회의 자료와 배포 점검표를 정리합니다',
        `https://example.test/${'long-document-title-'.repeat(6)}`,
      ];
      const documents: string[] = [];
      const user = str(await api.get('/api/me'), 'id');
      for (const title of titles) {
        const doc = await api.createWikiDocument(ws, title);
        documents.push(doc);
        const deleted = await page.request.delete(`/api/documents/${doc}`, {
          headers: { origin: new URL(page.url()).origin, 'x-fvoci-expected-user': user },
        });
        expect(deleted.status()).toBe(200);
      }
      await page.goto(`/w/${ws}/trash`);
      await expectViewport(page, narrow);
      if (narrow) {
        await expect(page.locator('html')).toHaveClass(/\bdark\b/);
        await page.addStyleTag({ content: 'html { font-size: 200%; }' });
      }
      const rows = page.getByTestId('trash-list').getByRole('listitem');
      await expect(rows).toHaveCount(titles.length);
      await page.evaluate(async () => {
        await document.fonts.ready;
      });
      const sizes = await Promise.all(
        titles.map((title) => rowSize(rows.filter({ hasText: title }))),
      );
      const gaps = await page
        .locator('#trash-retention, #version-retention')
        .evaluateAll((inputs) =>
          inputs.map((input) => {
            const button = input.closest('form')?.querySelector('button[type="submit"]');
            if (button === null || button === undefined)
              throw new Error('missing retention submit');
            return button.getBoundingClientRect().left - input.getBoundingClientRect().right;
          }),
        );
      const metrics = {
        viewport: narrow ? 375 : 1280,
        fontScale: narrow ? 200 : 100,
        rows: sizes.map((size) => ({ ...size, ratio: size.title / size.inside })),
        pageHeight: await page.evaluate(() => document.documentElement.scrollHeight),
        retentionGaps: gaps,
      };
      writeFileSync(testInfo.outputPath('layout.json'), JSON.stringify(metrics, null, 2));
      console.log(JSON.stringify(metrics));
      await page.screenshot({ path: testInfo.outputPath('trash.png'), fullPage: true });

      for (const [index, title] of titles.entries()) {
        const row = rows.filter({ hasText: title });
        const size = sizes[index];
        if (size === undefined) throw new Error('missing row measurement');
        const wrap = await measureWrap(row.locator(':scope > span').first());
        expect(wrap.overflow, 'title stays inside its box').toBe(false);
        await expect(row.getByRole('button')).toHaveText(['되살리기', '영구 삭제…']);
        if (narrow) {
          expect
            .soft(size.title / size.inside, 'title uses at least 80% of the row inside width')
            .toBeGreaterThanOrEqual(0.8);
          for (const action of size.actions)
            expect
              .soft(action.top, 'actions are below the title')
              .toBeGreaterThanOrEqual(size.titleBottom);
        } else {
          for (const action of size.actions) {
            expect(action.top, 'title and actions share a row').toBeLessThan(size.titleBottom);
            expect(action.bottom).toBeGreaterThan(size.titleTop);
          }
          expect(size.actions[0]?.left).toBeLessThan(size.actions[1]?.left ?? 0);
        }
      }
      if (!narrow) {
        expect(gaps).toHaveLength(2);
        expect
          .soft(Math.abs((gaps[0] ?? 0) - (gaps[1] ?? 0)), 'retention input/button gaps match')
          .toBeLessThanOrEqual(1);
      }
      expect(
        await pageOverflows(page),
        JSON.stringify(await offenders(page.locator('main').last())),
      ).toBe(false);

      const purgeRow = rows.filter({ hasText: titles[0] });
      const restore = purgeRow.getByRole('button', { name: '되살리기', exact: true });
      const purge = purgeRow.getByRole('button', { name: '영구 삭제…', exact: true });
      await restore.focus();
      await page.keyboard.press('Tab');
      await expect(purge).toBeFocused();
      await purge.click();
      const go = purgeRow.locator('[data-confirm-go]');
      await expect(go).toBeFocused();
      await expect(go).toHaveAccessibleDescription('되돌릴 수 없습니다. 영구 삭제할까요?');
      await expect(purgeRow.getByRole('button')).toHaveText(['되살리기', '영구 삭제', '취소']);
      if (narrow) {
        const size = await rowSize(purgeRow);
        expect(
          size.title / size.inside,
          'confirmation keeps the title wide',
        ).toBeGreaterThanOrEqual(0.8);
        for (const action of size.actions)
          expect(action.top, 'confirmation stays below the title').toBeGreaterThanOrEqual(
            size.titleBottom,
          );
      }
      expect(await pageOverflows(page)).toBe(false);
      await page.screenshot({ path: testInfo.outputPath('trash-confirm.png'), fullPage: true });
      await purgeRow.getByRole('button', { name: '취소', exact: true }).click();
      await expect(purge).toBeFocused();
      await expect(go).toHaveCount(0);
      await expect(rows).toHaveCount(titles.length);
      await purge.click();
      await expect(go).toBeFocused();
      await go.click();
      await expect(purgeRow).toHaveCount(0);
      expect(await api.status(`/api/documents/${documents[0] ?? ''}`)).toBe(404);

      const restoreRow = rows.filter({ hasText: titles[1] });
      await restoreRow.getByRole('button', { name: '되살리기', exact: true }).click();
      await expect(restoreRow).toHaveCount(0);
      expect(await api.status(`/api/documents/${documents[1] ?? ''}`)).toBe(200);
    });
  });
}
