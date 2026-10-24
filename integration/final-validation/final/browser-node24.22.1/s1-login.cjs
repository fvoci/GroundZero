const { launch, login, shot } = require('./lib.cjs');
(async () => {
  const b = await launch();
  const { page } = await login(b, 'u1');
  await shot(page, '01-u1-home');
  console.log('URL', page.url());
  const links = await page.getByRole('link').evaluateAll((els) => els.map((e) => `${e.textContent.trim()} -> ${e.getAttribute('href')}`));
  console.log(links.slice(0, 60).join('\n'));
  await b.close();
})().catch((e) => { console.error('FAIL', e.message); process.exit(1); });
