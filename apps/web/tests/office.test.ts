import { inlineSafe, officeKind } from '$lib/features/files/office';

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
    expect(officeKind('', '보고서.HWP')).toBe('hwp');
    expect(officeKind('application/haansofthwpx', 'a')).toBe('hwp');
    expect(officeKind('', 'form.hwpx')).toBe('hwp');
  });

  it('leaves other formats alone', () => {
    expect(officeKind('application/pdf', 'a.pdf')).toBeUndefined();
    expect(officeKind('', 'old.ppt')).toBeUndefined();
    expect(officeKind('', 'old.doc')).toBeUndefined();
  });
});

describe('pptx frame script inlining', () => {
  it('refuses scripts that would close the inline script element', () => {
    expect(inlineSafe('const a = 1 < 2;')).toBe(true);
    expect(inlineSafe('const a = "</script>";')).toBe(false);
    expect(inlineSafe('const a = "</SCRIPT >";')).toBe(false);
    expect(inlineSafe('const a = "<!-- x";')).toBe(false);
  });
});
