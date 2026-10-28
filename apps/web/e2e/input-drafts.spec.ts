// 실제 서버에서 댓글·체크리스트 초안의 명시 복원과 탭별 보존을 검증한다. 실행은 통합 브라우저 작업자가 맡는다.
import { expect, list, str, test, unique } from './fixtures';

test('reload offers long comment input and clears screen-only checklist input without sending either', async ({
  signIn,
}) => {
  const { page, api } = await signIn('test-user');
  const tag = unique();
  const ws = await api.createWorkspace(`입력 보존 ${tag}`);
  const item = await api.createProjectItem(ws, tag);
  const title = `입력 보존 업무 ${tag}`;
  const task = str(await api.createTask(item, title), 'id');
  await api.send('POST', '/api/comments', {
    request_id: crypto.randomUUID(),
    document_id: null,
    task_id: task,
    parent_id: null,
    body: '기존 댓글',
  });
  await api.send('POST', `/api/tasks/${task}/checklist`, {
    items: [{ request_id: crypto.randomUUID(), text: '기존 항목', checked: false }],
  });
  let unloadWarnings = 0;
  page.on('dialog', async (dialog) => {
    if (dialog.type() === 'beforeunload') unloadWarnings += 1;
    await dialog.accept();
  });
  await page.goto(`/w/${ws}/i/${item}/tasks/board/t/${task}`);
  const dialog = page.getByRole('dialog', { name: title, exact: true });
  await expect(dialog).toBeVisible();
  const comments = dialog.locator('section[aria-labelledby="comments-heading"]');
  const checklist = dialog.locator(`section[aria-labelledby="task-${task}-heading"]`);
  await comments.getByLabel('새 댓글(Ctrl·⌘+Enter로 달기)').fill('새 댓글 초안');
  await comments.getByRole('button', { name: '답글', exact: true }).click();
  await comments.getByRole('textbox', { name: '답글', exact: true }).fill('답글 초안');
  await comments.getByRole('button', { name: '고치기', exact: true }).click();
  await comments.getByLabel('댓글 고치기').fill('댓글 수정 초안');
  async function typeChecklist(): Promise<void> {
    await checklist.getByLabel('새 항목', { exact: true }).fill('새 체크리스트 항목');
    await checklist.getByRole('button', { name: '기존 항목 고치기', exact: true }).click();
    await checklist.getByLabel('기존 항목의 새 문구').fill('항목 수정 초안');
    await checklist.getByRole('button', { name: 'Markdown 붙여넣기' }).click();
    await checklist.getByLabel(/붙여넣을 체크리스트/).fill('- [ ] 원문');
    await checklist.getByRole('button', { name: '항목으로 펼치기' }).click();
    await checklist.getByLabel('항목 1 내용').fill('미리보기 수정 초안');
    await checklist.getByRole('checkbox', { name: '미리보기 수정 초안 완료 여부' }).check();
  }
  await typeChecklist();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          Object.keys(localStorage).filter(
            (key) => key.includes('form-draft:comment-') || key.includes('form-draft:checklist:'),
          ).length,
      ),
    )
    .toBe(3);

  await page.reload();
  await expect(dialog).toBeVisible();
  expect(unloadWarnings).toBeGreaterThan(0);
  await expect(comments.getByLabel('새 댓글(Ctrl·⌘+Enter로 달기)')).toHaveValue('');
  await expect(checklist.getByLabel('새 항목', { exact: true })).toHaveValue('');
  for (const testId of [
    'comment-draft-offer',
    'comment-reply-draft-offer',
    'comment-edit-draft-offer',
  ]) {
    await dialog.getByTestId(testId).getByRole('button', { name: '입력 되살리기' }).click();
  }
  await expect(comments.getByLabel('댓글 고치기')).toHaveValue('댓글 수정 초안');
  await expect(comments.getByRole('textbox', { name: '답글', exact: true })).toHaveValue(
    '답글 초안',
  );
  await expect(checklist.getByTestId('checklist-new-item-draft-offer')).toHaveCount(0);
  await expect(checklist.getByText('기존 항목', { exact: true })).toBeVisible();
  // 짧은 칸은 복원하지 않는다. 직접 다시 입력한 뒤 저장한다.
  await typeChecklist();
  expect(list(await api.get(`/api/tasks/${task}/comments`))).toHaveLength(1);
  expect(list(await api.get(`/api/checklist-items?task=${task}`))).toHaveLength(1);

  await comments.getByRole('button', { name: '댓글 달기', exact: true }).click();
  await expect(comments.getByLabel('새 댓글(Ctrl·⌘+Enter로 달기)')).toHaveValue('');
  await comments.getByRole('button', { name: '답글 달기', exact: true }).click();
  await expect(comments.getByRole('textbox', { name: '답글', exact: true })).toHaveCount(0);
  await comments.getByRole('button', { name: '저장', exact: true }).click();
  await expect(comments.getByLabel('댓글 고치기')).toHaveCount(0);
  await checklist.getByRole('button', { name: '추가', exact: true }).click();
  await expect(checklist.getByLabel('새 항목', { exact: true })).toHaveValue('');
  await checklist.getByRole('button', { name: '1개 덧붙이기', exact: true }).click();
  await expect(checklist.getByLabel('항목 1 내용')).toHaveCount(0);
  await checklist.getByRole('button', { name: '저장', exact: true }).click();
  await expect(checklist.getByLabel('기존 항목의 새 문구')).toHaveCount(0);
  expect(
    list(await api.get(`/api/tasks/${task}/comments`)).map((entry) => str(entry, 'body')),
  ).toEqual(expect.arrayContaining(['새 댓글 초안', '답글 초안', '댓글 수정 초안']));
  expect(
    list(await api.get(`/api/checklist-items?task=${task}`)).map((entry) => str(entry, 'text')),
  ).toEqual(expect.arrayContaining(['새 체크리스트 항목', '미리보기 수정 초안', '항목 수정 초안']));
});

test('two open tabs keep separate comment and checklist input when one tab saves', async ({
  signIn,
}) => {
  const { page, api } = await signIn('test-user');
  const tag = unique();
  const ws = await api.createWorkspace(`탭별 입력 ${tag}`);
  const item = await api.createProjectItem(ws, tag);
  const title = `탭별 업무 ${tag}`;
  const task = str(await api.createTask(item, title), 'id');
  const path = `/w/${ws}/i/${item}/tasks/board/t/${task}`;
  await page.goto(path);
  const second = await page.context().newPage();
  await second.goto(path);
  for (const [tab, name] of [
    [page, '첫 탭'],
    [second, '둘째 탭'],
  ] as const) {
    tab.on('dialog', (dialog) => {
      void dialog.accept();
    });
    const dialog = tab.getByRole('dialog', { name: title, exact: true });
    await expect(dialog).toBeVisible();
    await dialog.getByLabel('새 댓글(Ctrl·⌘+Enter로 달기)').fill(`${name} 댓글`);
    await dialog.getByLabel('새 항목', { exact: true }).fill(`${name} 항목`);
  }
  const firstDialog = page.getByRole('dialog', { name: title, exact: true });
  await firstDialog.getByRole('button', { name: '댓글 달기', exact: true }).click();
  await expect(firstDialog.getByLabel('새 댓글(Ctrl·⌘+Enter로 달기)')).toHaveValue('');
  await firstDialog.getByRole('button', { name: '추가', exact: true }).click();
  await expect(firstDialog.getByLabel('새 항목', { exact: true })).toHaveValue('');
  await second.reload();
  const secondDialog = second.getByRole('dialog', { name: title, exact: true });
  await expect(secondDialog.getByTestId('comment-draft-offer')).toContainText('둘째 탭 댓글');
  await expect(secondDialog.getByTestId('checklist-new-item-draft-offer')).toHaveCount(0);
  await expect(secondDialog.getByLabel('새 항목', { exact: true })).toHaveValue('');
  expect(
    list(await api.get(`/api/tasks/${task}/comments`)).map((entry) => str(entry, 'body')),
  ).toEqual(['첫 탭 댓글']);
  expect(
    list(await api.get(`/api/checklist-items?task=${task}`)).map((entry) => str(entry, 'text')),
  ).toEqual(['첫 탭 항목']);
  await second.close();
});
