const { launch, login, shot, BASE } = require('./lib.cjs');
const DOC = '/w/01a16d9d-7f83-7011-887f-0d334aa31e30/wiki/01a16d9d-7f83-7011-887f-0d39140cc6d4';
async function textOf(p) { return p.locator('.ProseMirror').first().innerText(); }
async function waitText(p, s, ms = 15000) { const t0 = Date.now(); while (Date.now() - t0 < ms) { if ((await textOf(p)).includes(s)) return Date.now() - t0; await p.waitForTimeout(200); } return -1; }
(async () => {
  const b = await launch();
  const a = await login(b, 'u1'); const c = await login(b, 'u2');
  const ws = { a: [], c: [] };
  a.page.on('websocket', (w) => ws.a.push(w.url())); c.page.on('websocket', (w) => ws.c.push(w.url()));
  await a.page.goto(BASE + DOC); await c.page.goto(BASE + DOC);
  await a.page.waitForLoadState('networkidle'); await c.page.waitForLoadState('networkidle'); await a.page.waitForTimeout(1500);
  console.log('ws u1:', ws.a.map((u) => u.replace(/\?.*/, '?…')), 'ws u2:', ws.c.map((u) => u.replace(/\?.*/, '?…')));
  console.log('live u1:', JSON.stringify((await a.page.locator('[aria-live]').allTextContents()).filter((s) => s.trim())));
  const m1 = `사용자1 실시간 ${Date.now()}`;
  await a.page.locator('.ProseMirror').first().click(); await a.page.keyboard.press('Control+End');
  await a.page.keyboard.press('Enter'); await a.page.keyboard.type(m1);
  console.log('u2 sees u1 text after ms:', await waitText(c.page, m1));
  const m2 = `사용자2 실시간 ${Date.now()}`;
  await c.page.locator('.ProseMirror').first().click(); await c.page.keyboard.press('Control+End');
  await c.page.keyboard.press('Enter'); await c.page.keyboard.type(m2);
  console.log('u1 sees u2 text after ms:', await waitText(a.page, m2));
  await a.page.waitForTimeout(1500);
  await shot(a.page, '08-collab-u1'); await shot(c.page, '09-collab-u2');
  console.log('live u1:', JSON.stringify((await a.page.locator('[aria-live]').allTextContents()).filter((s) => s.trim())));
  console.log('live u2:', JSON.stringify((await c.page.locator('[aria-live]').allTextContents()).filter((s) => s.trim())));
  await b.close();
})().catch((e) => { console.error('FAIL', e.message); process.exit(1); });
