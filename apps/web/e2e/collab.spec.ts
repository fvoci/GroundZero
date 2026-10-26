// 두 사용자(서로 다른 브라우저 컨텍스트, 쿠키 공유 없음)의 실시간 공동 편집과 저장 결과.
// 실시간 연결·동기화는 DB 저장이 아니다(docs/development.md 실시간 편집 보존). 저장 결과는 모두 나간 뒤 서버가 남긴
// 리비전을 API로 확인하고, 새 컨텍스트에서 다시 열어 본다.
import type { Page } from '@playwright/test';

import { expect, test, unique } from './fixtures';

const editor = (page: Page) => page.getByRole('textbox', { name: '본문' });

test('two users edit a live document together and the result persists', async ({ signIn }) => {
  const owner = await signIn('test-user');
  const writer = await signIn('test-user-2');
  const tag = unique();
  const ws = await owner.api.createWorkspace(`공동 편집 시험 ${tag}`);
  await owner.api.ensureOrganizationMember(writer.api, writer.user.email);
  await owner.api.invite(ws, writer.user.email, 'member');
  await writer.api.acceptInvitation(ws);
  await owner.api.setRealtime(ws, true);
  const doc = await owner.api.createWikiDocument(ws, `공동 편집 ${tag}`);
  const url = `/w/${ws}/wiki/${doc}`;

  await owner.page.goto(url);
  await writer.page.goto(url);
  for (const { page } of [owner, writer]) {
    await expect(page.getByRole('status').filter({ hasText: '실시간 연결됨' })).toBeVisible();
  }

  const first = `첫 번째 사용자가 쓴 문장입니다 ${tag}`;
  const second = `두 번째 사용자가 덧붙인 문장입니다 ${tag}`;
  await editor(owner.page).click();
  await owner.page.keyboard.type(first);
  await expect(editor(writer.page)).toContainText(first);

  await editor(writer.page).click();
  await writer.page.keyboard.press('Control+End');
  await writer.page.keyboard.press('Enter');
  await writer.page.keyboard.type(second);
  await expect(editor(owner.page)).toContainText(second);
  await expect(editor(writer.page)).toContainText(first);

  // 둘 다 나가면 방이 리비전으로 남긴다. 서버의 문서 본문에 두 사람의 문장이 모두 있어야 한다.
  await owner.page.goto(`/w/${ws}`);
  await writer.page.goto(`/w/${ws}`);
  await expect
    .poll(async () => JSON.stringify(await owner.api.get(`/api/documents/${doc}`)), {
      timeout: 30_000,
    })
    .toMatch(new RegExp(`${first}.*${second}`));

  // 브라우저 초안이 없는 새 컨텍스트에서 다시 연다.
  const reopened = await signIn('test-user-2');
  await reopened.page.goto(url);
  await expect(editor(reopened.page)).toContainText(first);
  await expect(editor(reopened.page)).toContainText(second);
});
