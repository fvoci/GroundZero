const { launch, login, shot, BASE } = require('./lib.cjs');
const ITEM = '/w/01a16d9d-801f-7299-ad3b-9378f8820e19/i/01a16d9d-801f-7299-ad3b-93812c2009be';
const FROM = process.argv[2] ?? '할 일'; const TO = process.argv[3] ?? '진행 중';
const col = (p, n) => p.locator(`section[aria-label="${n} 열"]`);
async function openBoard(p) { await p.goto(BASE + ITEM); await p.waitForLoadState('networkidle'); await p.getByRole('button', { name: '칸반', exact: true }).click(); await p.waitForTimeout(800); }
(async () => {
  const b = await launch();
  const { page } = await login(b, 'u1');
  await openBoard(page);
  const card = col(page, FROM).getByText('마일스톤: 검토 완료', { exact: true });
  await card.scrollIntoViewIfNeeded();
  const from = await card.boundingBox(); const to = await col(page, TO).boundingBox();
  const moves = [];
  page.on('response', (r) => { if (/\/move/.test(r.url())) moves.push(`${r.request().method()} ${r.status()}`); });
  await page.mouse.move(from.x + 20, from.y + 10); await page.mouse.down();
  await page.mouse.move(from.x + 60, from.y + 30, { steps: 5 });
  await page.mouse.move(to.x + to.width / 2, to.y + 120, { steps: 20 });
  await page.mouse.up();
  await page.waitForLoadState('networkidle'); await page.waitForTimeout(1500);
  console.log('move responses:', moves);
  await shot(page, '14-board-after-drag');
  await openBoard(page);
  const inTo = await col(page, TO).getByText('마일스톤: 검토 완료', { exact: true }).count();
  const inFrom = await col(page, FROM).getByText('마일스톤: 검토 완료', { exact: true }).count();
  console.log('after reload: in', TO, '=', inTo, ', in', FROM, '=', inFrom);
  await col(page, TO).scrollIntoViewIfNeeded(); await shot(page, '15-board-after-reload');
  await b.close();
})().catch((e) => { console.error('FAIL', e.message); process.exit(1); });
