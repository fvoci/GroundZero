// 한국어 문장이 어절 중간에서 줄을 바꾸지 않게 하는 규칙은 제목·문서 본문만이 아니라 앱의 모든 글자에 이어져야 한다
// (안내문·메뉴 항목이 "않습니 / 다."처럼 끊겼다, #112 검증). jsdom은 줄을 배치하지 않으므로 상속의 시작점인 body 규칙,
// 그 스타일시트를 앱이 실제로 불러오는지, 뿌리 쪽에서 되돌리는 규칙·클래스가 없는지를 확인한다.
import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';

const src = path.resolve(import.meta.dirname, '../src');
const read = (file: string): string => readFileSync(path.join(src, file), 'utf8');
const css = read('styles/app.css').replace(/\/\*[\s\S]*?\*\//g, '');

describe('Korean line breaking', () => {
  it('keeps Korean words together from body down, while long unbroken text can still wrap', () => {
    const body = /\n {2}body \{([^}]*)\}/.exec(css)?.[1];
    expect(body).toBeDefined();
    expect(body).toMatch(/word-break:\s*keep-all;/);
    expect(body).toMatch(/overflow-wrap:\s*anywhere;/);
  });

  it('is loaded by the app entry', () => {
    expect(read('app/main.ts')).toContain("import '../styles/app.css';");
  });

  it('is not reset on the root elements by another rule', () => {
    const rules = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map(
      ([, selector = '', decls = '']) => ({
        selector: selector.trim(),
        decls,
      }),
    );
    const root = /(^|,)\s*(html|body|:root|\*)\s*(,|$)/;
    const resets = rules.filter(
      (r) =>
        root.test(r.selector) &&
        [...r.decls.matchAll(/word-break:\s*([\w-]+)/g)].some(([, v]) => v !== 'keep-all'),
    );
    expect(resets).toEqual([]);
  });

  it('is not undone by word-break utility classes in components', () => {
    const files = readdirSync(src, { recursive: true, encoding: 'utf8' }).filter((f) =>
      f.endsWith('.svelte'),
    );
    const offenders = files.filter((f) => /\bbreak-(all|normal)\b/.test(read(f)));
    expect(offenders).toEqual([]);
  });
});
