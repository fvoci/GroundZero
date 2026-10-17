import { inlineScript, officeKind } from '$lib/features/files/office';

describe('office preview kinds', () => {
  it('recognises DOCX, XLSX and PPTX by type or extension', () => {
    expect(
      officeKind('application/vnd.openxmlformats-officedocument.wordprocessingml.document', 'a'),
    ).toBe('docx');
    expect(officeKind('application/octet-stream', '보고서.DOCX')).toBe('docx');
    expect(
      officeKind('application/vnd.openxmlformats-officedocument.spreadsheetml.sheet', 'b'),
    ).toBe('xlsx');
    expect(officeKind('', 'data.xlsx')).toBe('xlsx');
    expect(
      officeKind('application/vnd.openxmlformats-officedocument.presentationml.presentation', 'c'),
    ).toBe('pptx');
    expect(officeKind('', 'slides.PPTX')).toBe('pptx');
  });

  it('leaves other formats alone', () => {
    expect(officeKind('application/pdf', 'a.pdf')).toBeUndefined();
    expect(officeKind('', 'old.ppt')).toBeUndefined();
    expect(officeKind('', 'old.doc')).toBeUndefined();
  });
});

describe('pptx frame script inlining', () => {
  it('breaks closing tags and comment openers without changing the script', () => {
    const code = 'const a = "</script><!-- x"; const b = "</SCRIPT>";';
    const inlined = inlineScript(code);
    expect(inlined).not.toMatch(/<\/script/i);
    expect(inlined).not.toContain('<!--');
    // 자바스크립트 문자열로는 같은 값이다(`\/`와 `\!`는 그 글자 그대로).
    expect(inlined).toBe(String.raw`const a = "<\/script><\!-- x"; const b = "<\/SCRIPT>";`);
  });
});
