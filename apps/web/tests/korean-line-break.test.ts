// 한국어 문장이 어절 중간에서 줄을 바꾸지 않게 하는 규칙은 제목·문서 본문만이 아니라 앱의 모든 글자에 이어져야 한다
// (안내문·메뉴 항목이 "않습니 / 다."처럼 끊겼다, #112 검증). jsdom은 줄을 배치하지 않으므로 상속의 시작점인 body 규칙을 확인한다.
import { readFileSync } from 'node:fs';
import path from 'node:path';

const css = readFileSync(path.resolve(import.meta.dirname, '../src/styles/app.css'), 'utf8');

describe('Korean line breaking', () => {
  it('keeps Korean words together from body down, while long unbroken text can still wrap', () => {
    const body = /\n {2}body \{([^}]*)\}/.exec(css)?.[1];
    expect(body).toBeDefined();
    expect(body).toMatch(/word-break:\s*keep-all;/);
    expect(body).toMatch(/overflow-wrap:\s*anywhere;/);
  });
});
