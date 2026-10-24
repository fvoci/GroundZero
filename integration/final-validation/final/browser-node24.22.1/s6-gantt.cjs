const { launch, login, shot, BASE } = require('./lib.cjs');
const ITEM = '/w/01a16d9d-801f-7299-ad3b-9378f8820e19/i/01a16d9d-801f-7299-ad3b-93812c2009be';
(async () => {
  const b = await launch();
  const { page } = await login(b, 'u1');
  await page.goto(BASE + ITEM); await page.waitForLoadState('networkidle');
  const tab = page.getByRole('button', { name: '간트', exact: true });
  console.log('간트 buttons:', await tab.count());
  await tab.click(); await page.waitForTimeout(1500);
  console.log('pressed:', await tab.getAttribute('aria-pressed'), await tab.getAttribute('aria-selected'));
  await tab.scrollIntoViewIfNeeded(); await page.mouse.wheel(0, 250); await page.waitForTimeout(500);
  const heads = await page.locator('[role=columnheader], th, [role=rowheader]').count();
  console.log('headers:', heads, '| week text:', await page.getByText(/주차|W\d+|\d+\.\s?\d+\.\s?~/).count());
  await shot(page, '16-gantt');
  await b.close();
})().catch((e) => { console.error('FAIL', e.message); process.exit(1); });
