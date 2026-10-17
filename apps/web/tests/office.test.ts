import { officeKind } from '$lib/features/files/office';

describe('office preview kinds', () => {
  it('recognises DOCX and XLSX by type or extension', () => {
    expect(
      officeKind('application/vnd.openxmlformats-officedocument.wordprocessingml.document', 'a'),
    ).toBe('docx');
    expect(officeKind('application/octet-stream', '보고서.DOCX')).toBe('docx');
    expect(
      officeKind('application/vnd.openxmlformats-officedocument.spreadsheetml.sheet', 'b'),
    ).toBe('xlsx');
    expect(officeKind('', 'data.xlsx')).toBe('xlsx');
  });

  it('leaves other formats alone', () => {
    expect(officeKind('application/pdf', 'a.pdf')).toBeUndefined();
    expect(officeKind('', 'slides.pptx')).toBeUndefined();
    expect(officeKind('', 'old.doc')).toBeUndefined();
  });
});
