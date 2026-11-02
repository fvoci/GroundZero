import seed from '../../../deploy/postgres/dev-ui-data.json' with { type: 'json' };
import { expect, listField, str, test, unique } from './fixtures';

test('measures the item checklist collection while opening a seeded board (Q2)', async ({
  signIn,
}, testInfo) => {
  const { page, api } = await signIn('test-user');
  const tag = unique();
  const workspace = await api.createWorkspace(`Q2 seed ${tag}`);
  const item = await api.createProjectItem(workspace, tag);
  let first: string | undefined;
  for (const row of seed.tasks) {
    const task = await api.createTask(item, row.title, {
      description: row.description,
      priority: row.priority,
    });
    first ??= str(task, 'id');
  }
  // dev-module-seed.sql의 대표 업무와 같은 체크리스트 두 줄.
  if (first === undefined) throw new Error('seed has no tasks');
  await api.send('POST', `/api/tasks/${first}/checklist`, {
    items: [
      { request_id: crypto.randomUUID(), text: '준비 자료 확인', checked: true },
      { request_id: crypto.randomUUID(), text: '저장 후 다시 열기', checked: false },
    ],
  });
  const checklistRequests: string[] = [];
  page.on('request', (request) => {
    const url = new URL(request.url());
    if (url.pathname === '/api/checklist-items') checklistRequests.push(request.url());
  });
  const collected = page.waitForResponse((response) => {
    const url = new URL(response.url());
    return (
      url.pathname === `/api/items/${item}/task-board` && response.request().method() === 'GET'
    );
  });
  await page.goto(`/w/${workspace}/i/${item}/tasks/board`);
  const response = await collected;
  const body = await response.body();
  const timing = response.request().timing();
  const value: unknown = JSON.parse(body.toString());
  const measurement = {
    seed_tasks: seed.tasks.length,
    checklist_rows: listField(value, 'checklist').length,
    decoded_bytes: body.length,
    response_ms: timing.responseEnd - timing.requestStart,
  };
  expect(response.status()).toBe(200);
  expect(measurement.checklist_rows).toBe(2);
  await expect(page.getByTestId('task-board')).toBeVisible();
  expect(checklistRequests).toEqual([]);
  console.log(`Q2_CHECKLIST_READ ${JSON.stringify(measurement)}`);
  await testInfo.attach('q2-checklist-read.json', {
    body: JSON.stringify(measurement),
    contentType: 'application/json',
  });
});
