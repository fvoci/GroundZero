// D-86 / #214: deployment contracts, independent of the generator.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { test } from 'node:test';

const root = new URL('../', import.meta.url);
const css = readFileSync(new URL('apps/web/src/styles/app.css', root), 'utf8');
const html = readFileSync(new URL('apps/web/index.html', root), 'utf8');

test('only eight WOFF2 derivatives and the two original licenses ship', () => {
  const files = readdirSync(new URL('apps/web/src/assets/fonts/', root)).sort();
  assert.deepEqual(
    files,
    [
      'LICENSE-Noto-CJK.txt',
      'OFL-D2Coding.txt',
      ...['Regular', 'Bold'].flatMap((weight) => [
        `NotoSansKR-${weight}-common.woff2`,
        `NotoSansKR-${weight}-rest.woff2`,
        `NotoSerifKR-${weight}.woff2`,
        `D2Coding-${weight}.woff2`,
      ]),
    ].sort(),
  );
});

test('first screen preloads only the common piece of each Sans weight', () => {
  const hints = [...html.matchAll(/<link\b[^>]*rel="preload"[^>]*>/g)].map(([tag]) => tag);
  assert.equal(hints.length, 2);
  for (const weight of ['Regular', 'Bold']) {
    assert.equal(
      hints.filter((tag) => tag.includes(`NotoSansKR-${weight}-common.woff2`)).length,
      1,
    );
  }
  for (const tag of hints) {
    assert.match(tag, /as="font"/);
    assert.match(tag, /type="font\/woff2"/);
    assert.match(tag, /crossorigin/);
  }
  assert.doesNotMatch(css, /\.(otf|ttf)['"]/);
});
