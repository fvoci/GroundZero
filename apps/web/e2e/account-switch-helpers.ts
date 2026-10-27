// 계정 전환 e2e 공통: 같은 컨텍스트의 다른 페이지에서 계정을 바꾸고, 원래 창으로 돌아와 감지를 기다린다.
import { appendFileSync } from 'node:fs';

import type { Page, TestInfo } from '@playwright/test';

import { expect, logIn, recordSensitive, str, testUser } from './fixtures';

/** 이 사용자의 업무 폼 초안 키(브라우저 저장소). */
export async function draftKeys(page: Page, userId: string): Promise<string[]> {
  return page.evaluate(
    (prefix) => Object.keys(window.localStorage).filter((k) => k.startsWith(prefix)),
    `fvoci:${userId}:-:form-draft:`,
  );
}

/** 다른 페이지에서 로그아웃하고 다른 계정으로 로그인한다(쿠키는 컨텍스트 공용). 새 계정의 id를 돌려준다. */
export async function switchTo(page: Page, username: 'test-user' | 'test-user-2'): Promise<string> {
  await page.goto('/');
  await page.getByTestId('user-menu').click();
  await page.getByRole('menuitem', { name: '로그아웃' }).click();
  await expect(page.getByRole('link', { name: '로그인' })).toBeVisible();
  await logIn(page, testUser(username));
  recordSensitive((await page.context().cookies()).map((c) => c.value));
  return str(await (await page.request.get('/api/me')).json(), 'id');
}

/**
 * 창으로 돌아온다. 헤드리스는 탭 전환이 감지 이벤트(focus·visibilitychange)를 내지 않으므로 자연 감지가 없으면 focus를 직접
 * 낸다(그 뒤의 확인·전환·다시 읽기는 실제 코드). 어느 경로였는지 `focus-path.txt`에 남긴다. `arm`은 감지가 일어났는지를
 * timeout 안에 판정한다.
 */
export async function returnToWindow(
  page: Page,
  testInfo: TestInfo,
  arm: (timeout: number) => Promise<boolean>,
): Promise<void> {
  const note = (text: string): void => {
    testInfo.annotations.push({ type: 'focus', description: text });
    appendFileSync(testInfo.outputPath('focus-path.txt'), `${text}\n`);
  };
  const natural = arm(5_000);
  await page.bringToFront();
  if (await natural) {
    note('bringToFront fired a detection event (focus or visibilitychange)');
    return;
  }
  note('headless: window focus dispatched programmatically (not a real tab switch)');
  const forced = arm(20_000);
  await page.evaluate(() => {
    window.dispatchEvent(new Event('focus'));
  });
  expect(await forced, 'detection after a dispatched focus event').toBe(true);
}
