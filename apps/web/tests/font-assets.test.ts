// D-86 / #214: deployment contracts, independent of the generator.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { test } from 'vitest';

const root = path.resolve(import.meta.dirname, '../../..');
const css = readFileSync(path.join(root, 'apps/web/src/styles/app.css'), 'utf8');
const html = readFileSync(path.join(root, 'apps/web/index.html'), 'utf8');

test('only six Noto WOFF2 derivatives, two original D2Coding TTFs and licenses ship', () => {
  const files = readdirSync(path.join(root, 'apps/web/src/assets/fonts')).sort();
  assert.deepEqual(
    files,
    [
      'LICENSE-Noto-CJK.txt',
      'OFL-D2Coding.txt',
      ...['Regular', 'Bold'].flatMap((weight) => [
        `NotoSansKR-${weight}-common.woff2`,
        `NotoSansKR-${weight}-rest.woff2`,
        `NotoSerifKR-${weight}.woff2`,
        `D2Coding-${weight}.ttf`,
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
  assert.doesNotMatch(css, /\.otf['"]/);
  for (const weight of ['Regular', 'Bold']) {
    assert.match(css, new RegExp(`D2Coding-${weight}\\.ttf`));
  }
});
