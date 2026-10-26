// 되살리기 제안(업무 상세·보드)의 배치: 1280px·375px, 글꼴 100%·200%(Storybook과 같은 루트 글자 크기 배율). 넘침은 헤드리스
// 단언이고, 색·간격·겹침은 남긴 캡처를 사람이 본다(OS 글꼴 배율·실제 확대·실기기는 아니다).
import { appendFileSync } from 'node:fs';

import type { Locator } from '@playwright/test';

import { expect, str, test, unique } from './fixtures';
import { expectViewport, pageOverflows } from './layout';
import { offenders } from './offenders';

async function innerOverflow(target: Locator): Promise<{ scroll: number; client: number }> {
  return target.evaluate((el) => ({ scroll: el.scrollWidth, client: el.clientWidth }));
}

for (const scale of [100, 200] as const) {
  test(`draft offers fit the dialog and the board at font scale ${String(scale)}%`, async ({
    signIn,
  }, testInfo) => {
    const { page, api } = await signIn('test-user');
    const narrow = testInfo.project.name === 'narrow-375';
    const tag = unique();
    const ws = await api.createWorkspace(`제안 배치 ${tag}`);
    const item = await api.createProjectItem(ws, tag);
    const title = `제안 배치 업무 ${tag}`;
    const taskId = str(await api.createTask(item, title), 'id');
    const boardPath = `/w/${ws}/i/${item}/tasks/board`;
    const dialog = page.getByRole('dialog', { name: title, exact: true });
    const offer = page.getByTestId('task-draft-offer');
    const newOffer = page.getByTestId('task-new-draft-offer');
    const applyScale = async (): Promise<void> => {
      if (scale !== 100)
        await page.addStyleTag({ content: `html { font-size: ${String(scale)}%; }` });
      await page.evaluate(async () => {
        await document.fonts.ready;
      });
    };
    // 새로고침 전 경고는 떠나기로 답한다(입력은 브라우저에 남는다).
    page.on('dialog', (prompt) => {
      void prompt.accept();
    });

    await page.goto(boardPath);
    await expectViewport(page, narrow);
    await page.getByRole('textbox', { name: '새 업무 제목', exact: true }).fill(`새 제목 ${tag}`);
    await page.locator(`[data-task-card="${taskId}"]`).click();
    await expect(dialog).toBeVisible();
    await dialog
      .getByLabel('설명', { exact: true })
      .fill(`긴 설명을 저장하지 않고 두었습니다 ${tag}`);
    await expect
      .poll(() =>
        page
          .evaluate(() => Object.keys(window.localStorage).filter((k) => k.includes('form-draft')))
          .then((keys) => keys.length),
      )
      .toBe(2);
    await page.reload();
    await applyScale();
    await expect(dialog).toBeVisible();
    await expect(offer).toBeVisible();
    const dialogBox = await innerOverflow(dialog);
    expect(
      dialogBox.scroll,
      `dialog inner width; offenders=${JSON.stringify(await offenders(dialog))}`,
    ).toBeLessThanOrEqual(dialogBox.client);
    const offerBox = await innerOverflow(offer);
    expect(offerBox.scroll, 'task offer width').toBeLessThanOrEqual(offerBox.client);
    await page.screenshot({ path: testInfo.outputPath(`task-offer-${String(scale)}.png`) });

    // 제안을 두고 닫아도(고르지 않음) 입력은 남고 보드의 새 업무 제안이 보인다.
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
    await expect(newOffer).toBeVisible();
    // 제안 상자는 자기 안에서도, 화면 폭 안에서도 넘치지 않아야 한다. 페이지 전체의 가로 넘침은 이 제안과 무관한 요소도
    // 포함하므로 기록만 한다(넘친 요소를 주석으로 남긴다).
    const newBox = await innerOverflow(newOffer);
    expect(newBox.scroll, 'board offer width').toBeLessThanOrEqual(newBox.client);
    const viewportWidth = page.viewportSize()?.width ?? 0;
    const offerRight = await newOffer.evaluate((el) =>
      Math.round(el.getBoundingClientRect().right),
    );
    expect(offerRight, 'board offer inside the viewport').toBeLessThanOrEqual(viewportWidth);
    if (await pageOverflows(page)) {
      const note = `${String(scale)}% ${narrow ? '375' : '1280'}: ${JSON.stringify(await offenders(page.locator('body')))}`;
      testInfo.annotations.push({ type: 'page-overflow', description: note });
      appendFileSync(
        testInfo.outputPath('page-overflow.txt'),
        `${note}
`,
      );
    }
    await page.screenshot({
      path: testInfo.outputPath(`board-offer-${String(scale)}.png`),
      fullPage: true,
    });
  });
}
