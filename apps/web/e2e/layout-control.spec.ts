// 줄바꿈 검사 자체의 음성 대조(375px viewport에서만 돈다). 예전 규칙(word-break: normal, #114 이전)을 이 실행의 페이지에만
// 덧씌우면 같은 검사가 어절 끊김을 찾아야 한다. 제품 코드는 바꾸지 않는다.
import { expect, test } from './fixtures';
import { expectViewport, measureWrap } from './layout';

test('detector reports words split under the old word-break rule', async ({ anonymous }) => {
  const page = await anonymous();
  await page.goto('/');
  await expectViewport(page, true);
  await page.addStyleTag({ content: 'body { word-break: normal !important; }' });
  const wrap = await measureWrap(page.getByText('작업·지식·학습을 한곳에서 관리합니다.'));
  expect(wrap.lines).toBeGreaterThan(1);
  expect(wrap.broken.length).toBeGreaterThan(0);
});
