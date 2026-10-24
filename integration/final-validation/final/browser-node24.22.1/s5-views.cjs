const { launch, login, shot, BASE } = require('./lib.cjs');
const WS = '/w/01a16d9d-801f-7299-ad3b-9378f8820e19';
const ITEM = WS + '/i/01a16d9d-801f-7299-ad3b-93812c2009be';
(async () => {
  const b = await launch();
  const { page } = await login(b, 'u1');
  await page.goto(BASE + ITEM); await page.waitForLoadState('networkidle');
  await page.getByRole('button', { name: '간트', exact: true }).click(); await page.waitForTimeout(1200);
  const gantt = page.locator('svg, [role=grid], table').filter({ hasText: /DEVP|10월|주/ }).first();
  await gantt.scrollIntoViewIfNeeded().catch(() => {});
  console.log('gantt bars mentioning DEVP:', await page.getByText(/DEVP-\d+/).count());
  await shot(page, '16-gantt');
  await page.getByRole('button', { name: '달력', exact: true }).click(); await page.waitForTimeout(1200);
  await shot(page, '17-item-calendar');
  await page.goto(BASE + WS + '/calendar'); await page.waitForLoadState('networkidle'); await page.waitForTimeout(1000);
  await shot(page, '18-ws-calendar');
  // narrow screen
  const n = await login(b, 'u2', { width: 390, height: 844 });
  await n.page.goto(BASE + ITEM); await n.page.waitForLoadState('networkidle'); await n.page.waitForTimeout(800);
  const sw = await n.page.evaluate(() => [document.documentElement.scrollWidth, window.innerWidth]);
  console.log('narrow item scrollWidth/innerWidth:', sw);
  await shot(n.page, '19-narrow-item');
  await n.page.goto(BASE + '/w/01a16d9d-7f83-7011-887f-0d334aa31e30/wiki/01a16d9d-7f83-7011-887f-0d39140cc6d4'); await n.page.waitForLoadState('networkidle'); await n.page.waitForTimeout(1200);
  console.log('narrow doc scrollWidth/innerWidth:', await n.page.evaluate(() => [document.documentElement.scrollWidth, window.innerWidth]));
  await shot(n.page, '20-narrow-doc');
  await b.close();
})().catch((e) => { console.error('FAIL', e.message); process.exit(1); });
