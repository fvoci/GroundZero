// 저장 중 잠금(F-16 이후 `<fieldset disabled>`)과 키보드 초점(R5 2차 delta 4묶음 확인 필요 1). 입력 칸에서 Enter로 제출하면
// 그 칸이 든 fieldset이 곧바로 비활성이 되어 초점이 body로 빠질 수 있다. 저장이 끝나 잠금이 풀리면 초점이 그 칸으로
// 돌아와야 키보드 사용자가 위치를 잃지 않는다. 저장 뒤에도 폼이 그대로 남는 강의 회차 추가에서 본다(멤버 초대는 성공 뒤
// 워크스페이스 목록을 다시 읽어 화면 전체를 새로 그리므로 입력 요소가 바뀐다). 요청은 대신 응답하지 않고 붙잡았다가
// 실제 서버로 보낸다.
import { expect, str, test, unique } from './fixtures';

/** 지금 초점 요소의 id(없으면 태그 이름). */
const focused = 'document.activeElement?.id || document.activeElement?.tagName';

test('focus returns to the episode title after an Enter submit is saved', async ({ signIn }) => {
  const { page, api } = await signIn('test-user');
  const tag = unique();
  const ws = await api.createWorkspace(`초점 ${tag}`);
  const module = await api.send('POST', `/api/workspaces/${ws}/modules`, {
    request_id: crypto.randomUUID(),
    kind: 'course',
    name: `시험 강의 ${tag}`,
    key: `C${tag
      .replace(/[^a-z0-9]/gi, '')
      .slice(0, 5)
      .toUpperCase()}`,
  });
  const item = str(
    await api.send('POST', `/api/modules/${str(module, 'id')}/items`, {
      request_id: crypto.randomUUID(),
      title: `시험 강의 항목 ${tag}`,
    }),
    'id',
  );

  let release: (() => void) | undefined;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  let held = false;
  await page.route(`**/api/items/${item}/episodes`, async (route) => {
    if (route.request().method() !== 'POST') return route.continue();
    held = true;
    await gate;
    await route.continue();
  });

  await page.goto(`/w/${ws}/i/${item}/episodes`);
  const title = page.locator('#episode-title');
  await expect(title).toBeVisible();
  await page.locator('#episode-duration').fill('12:30');
  const episode = `행렬 ${tag}`;
  await title.fill(episode);
  await title.press('Enter');

  await expect.poll(() => held).toBe(true);
  await expect(title).toBeDisabled();
  const whileSaving = await page.evaluate(focused);
  console.log(`focus while saving: ${String(whileSaving)}`);

  release?.();
  await expect(title).toBeEnabled();
  // 추가한 회차가 목록에 보인 뒤(다시 읽기까지 끝난 뒤)에도 초점이 제목 칸에 있어야 한다.
  await expect(page.getByText(episode, { exact: true })).toBeVisible();
  const afterSave = await page.evaluate(focused);
  console.log(`focus after save: ${String(afterSave)}`);
  expect(afterSave).toBe('episode-title');
});

// 저장에 성공하면 닫히는 대화상자. 닫힘 전환(100ms) 동안 내용이 남아 잠금이 풀리므로, 닫히는 대화상자 안의 입력으로
// 초점을 되돌리지 않는지 본다(초점 복귀는 대화상자 몫).
test('a status dialog that closes on success does not get focus back', async ({ signIn }) => {
  const { page, api } = await signIn('test-user');
  const tag = unique();
  const ws = await api.createWorkspace(`초점 대화상자 ${tag}`);
  const item = await api.createProjectItem(ws, tag);

  let release: (() => void) | undefined;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  let held = false;
  await page.route(`**/api/items/${item}/statuses`, async (route) => {
    if (route.request().method() !== 'POST') return route.continue();
    held = true;
    await gate;
    await route.continue();
  });

  await page.goto(`/w/${ws}/i/${item}/tasks/board`);
  await page.getByRole('button', { name: '상태 편집', exact: true }).click();
  await page.getByRole('button', { name: '상태 추가', exact: true }).click();
  const dialog = page.getByRole('dialog');
  const name = dialog.getByLabel('상태 이름', { exact: true });
  await name.fill(`검토 ${tag}`);
  await name.press('Enter');

  await expect.poll(() => held).toBe(true);
  await expect(name).toBeDisabled();
  // 잠금이 풀린 뒤 이 입력이 다시 초점을 받는지 남긴다.
  await name.evaluate((el) => {
    el.addEventListener('focus', () => {
      document.body.dataset.refocused = 'yes';
    });
  });

  release?.();
  await expect(dialog).toBeHidden();
  const refocused = await page.evaluate(() => document.body.dataset.refocused ?? 'no');
  const after = await page.evaluate(() => {
    const el = document.activeElement;
    if (el === null) return 'null';
    const text = el.textContent.trim();
    return text === '' ? el.tagName : text;
  });
  console.log(`closing dialog refocused: ${refocused}; focus after close: ${after}`);
  expect(refocused).toBe('no');
});
