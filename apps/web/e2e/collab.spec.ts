// 두 사용자(서로 다른 브라우저 컨텍스트, 쿠키 공유 없음)의 실시간 공동 편집과 저장 결과.
// 실시간 연결·동기화는 DB 저장이 아니다. 저장 결과는 모두 나간 뒤 서버가 남긴
// 리비전을 API로 확인하고, 새 컨텍스트에서 다시 열어 본다.
import type { Page } from '@playwright/test';
import type { Editor } from '@tiptap/core';

import { expect, str, test, unique } from './fixtures';

const editor = (page: Page) => page.getByRole('textbox', { name: '본문' });

test('two users edit a live document together and the result persists', async ({ signIn }) => {
  const owner = await signIn('test-user');
  const writer = await signIn('test-user-2');
  const tag = unique();
  const ws = await owner.api.createWorkspace(`공동 편집 시험 ${tag}`);
  await owner.api.ensureOrganizationMember(writer.api, writer.user.email);
  await owner.api.invite(ws, writer.user.email, 'member');
  await writer.api.acceptInvitation(ws);
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
  // 원격 본문 표시·키 전송 완료는 selectionchange의 상태 반영 완료가 아니다.
  // Enter가 쓰는 ProseMirror 선택과 브라우저 선택이 모두 문서 끝인지 확인한다.
  await expect
    .poll(() =>
      editor(writer.page).evaluate((element) => {
        const tiptap = element.closest<HTMLElement & { editor: Editor }>('.tiptap');
        if (tiptap === null) return false;
        const { state, view } = tiptap.editor;
        const selection = window.getSelection();
        const end = state.doc.content.size - 1;
        return (
          view.hasFocus() &&
          state.selection.empty &&
          state.selection.to === end &&
          selection?.isCollapsed === true &&
          selection.focusNode !== null &&
          element.contains(selection.focusNode) &&
          view.posAtDOM(selection.focusNode, selection.focusOffset) === end
        );
      }),
    )
    .toBe(true);
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

test('a stored body draft is copied to a private note through a realtime room', async ({
  signIn,
}) => {
  const owner = await signIn('test-user');
  const tag = unique();
  const ws = await owner.api.createWorkspace(`초안 보존 ${tag}`);
  const doc = await owner.api.createWikiDocument(ws, `원본 ${tag}`);
  const originalUrl = `/w/${ws}/wiki/${doc}`;
  await owner.page.goto(originalUrl);
  await expect(owner.page.getByRole('status').filter({ hasText: '실시간 연결됨' })).toBeVisible();
  const userId = str(await owner.api.get('/api/me'), 'id');
  const text = `이 브라우저에 보관한 초안 ${tag}`;
  await owner.page.evaluate(
    ({ userId, doc, text }) => {
      localStorage.setItem(
        `fvoci:${userId}:-:draft:${doc}:offer:000000000000001.test`,
        JSON.stringify({
          baseRevision: 1,
          title: '보관한 초안',
          savedAt: new Date().toISOString(),
          body: {
            type: 'doc',
            content: [{ type: 'paragraph', content: [{ type: 'text', text }] }],
          },
        }),
      );
    },
    { userId, doc, text },
  );
  await owner.page.reload();
  const offer = owner.page.getByRole('region', { name: '보관된 초안' });
  await expect(offer).toBeVisible();
  await offer.getByRole('button', { name: '내 초안을 내 노트로 보존', exact: true }).click();
  await expect(owner.page).toHaveURL(/\/d\/[^/]+$/);
  const noteId = new URL(owner.page.url()).pathname.split('/').at(-1);
  if (noteId === undefined) throw new Error('missing copied note id');
  expect(noteId).not.toBe(doc);
  await expect
    .poll(async () => JSON.stringify(await owner.api.get(`/api/documents/${noteId}`)))
    .toContain(text);
  await owner.page.reload();
  await expect(editor(owner.page)).toContainText(text);
  expect(JSON.stringify(await owner.api.get(`/api/documents/${doc}`))).not.toContain(text);
  await owner.page.goto(originalUrl);
  await expect(owner.page.getByRole('status').filter({ hasText: '실시간 연결됨' })).toBeVisible();
  await expect(owner.page.getByRole('region', { name: '보관된 초안' })).toHaveCount(0);
});
