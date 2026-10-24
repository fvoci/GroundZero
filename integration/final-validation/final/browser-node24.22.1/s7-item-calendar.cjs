const { launch, login, shot, BASE } = require('./lib.cjs');
const WS = '/w/01a16d9d-801f-7299-ad3b-9378f8820e19';
const ITEM = WS + '/i/01a16d9d-801f-7299-ad3b-93812c2009be';
(async () => {
  const b = await launch();
  const { page } = await login(b, 'u1');
  await page.goto(BASE + ITEM); await page.waitForLoadState('networkidle');
  await page.getByRole('button', { name: '달력', exact: true }).click(); await page.waitForTimeout(1200);
  // 달력 아래쪽 주(10/25–10/31)에 일정이 있다. 그 주가 화면 가운데 오도록 스크롤한다.
  const target = page.getByText(/DEVP-5/).filter({ visible: true }).last();
  await target.scrollIntoViewIfNeeded(); await page.evaluate(() => window.scrollBy(0, 200));
  await page.waitForTimeout(400);
  const visible = await page.getByText(/DEVP-\d+/).evaluateAll((els) => els
    .filter((e) => { const r = e.getBoundingClientRect(); return r.bottom > 0 && r.top < window.innerHeight && r.width > 0; })
    .map((e) => e.textContent.trim().replace(/\s+/g, ' ').slice(0, 40)));
  console.log('visible DEVP entries:', JSON.stringify(visible));
  await shot(page, '21-item-calendar-scrolled');
  await b.close();
})().catch((e) => { console.error('FAIL', e.message); process.exit(1); });
