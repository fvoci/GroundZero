// 배치 진단 도우미 자체의 회귀 시험(CodeRabbit #141 e5e001a5): 넘친 요소 가운데 SVG가 있어도 예외 없이 보고한다
// (SVG의 className은 SVGAnimatedString이고 innerText가 없다).
import { expect, test } from './fixtures';
import { offenders } from './offenders';

test('offenders() reports overflowing SVG elements instead of throwing', async ({ page }) => {
  await page.setContent(
    '<div id="box" style="width:100px"><svg class="lucide icon" style="display:block;width:200px;height:10px"></svg><span style="display:inline-block;width:150px">긴 글</span></div>',
  );
  const found = await offenders(page.locator('#box'));
  expect(found.map((f) => f.tag).sort()).toEqual(['span', 'svg']);
  expect(found.find((f) => f.tag === 'svg')?.cls).toBe('lucide icon');
  expect(found.find((f) => f.tag === 'span')?.text).toBe('긴 글');
});
