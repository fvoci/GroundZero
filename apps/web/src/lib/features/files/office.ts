// Office 미리보기(D-05, D-71 브라우저 우선). DOCX는 docx-preview(Apache-2.0)로 쪽 단위 HTML을 만들어 스크립트를 돌리지
// 못하는 iframe(sandbox)에 넣고, XLSX는 read-excel-file(MIT)로 시트별 값 표를 만든다. 둘 다 필요할 때만 불러온다.
// PPTX는 쓸 만한 브라우저 렌더러가 미승인 라이선스(0BSD·MPL-2.0)를 끌어와 아직 넣지 않았다(docs/development.md 57절).

export type OfficeKind = 'docx' | 'xlsx';

/** 미리볼 수 있는 최대 크기. 더 크면 내려받아 연다. */
export const OFFICE_PREVIEW_MAX_BYTES = 20 * 1024 * 1024;

export function officeKind(contentType: string, name: string): OfficeKind | undefined {
  const type = contentType.split(';')[0]?.trim().toLowerCase() ?? '';
  const ext = name.toLowerCase().split('.').pop() ?? '';
  if (
    type === 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' ||
    ext === 'docx'
  )
    return 'docx';
  if (
    type === 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet' ||
    ext === 'xlsx'
  )
    return 'xlsx';
  return undefined;
}

/** iframe 안에서 바깥으로 나가는 요청(추적 그림 등)을 막는다. 그림·글꼴은 문서 안의 것(data:)만. */
const FRAME_CSP = "default-src 'none'; img-src data:; style-src 'unsafe-inline'; font-src data:";

/** DOCX를 쪽 단위 HTML로 만든 iframe srcdoc. */
export async function docxSrcdoc(bytes: ArrayBuffer): Promise<string> {
  const { renderAsync } = await import('docx-preview');
  const body = document.createElement('div');
  const styles = document.createElement('div');
  await renderAsync(bytes, body, styles, {
    inWrapper: true,
    breakPages: true,
    ignoreLastRenderedPageBreak: true,
    useBase64URL: true,
    renderHeaders: true,
    renderFooters: true,
    renderFootnotes: true,
    renderEndnotes: true,
  });
  return `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${FRAME_CSP}">${styles.innerHTML}</head><body style="margin:0">${body.innerHTML}</body></html>`;
}

export type CellValue = string | number | boolean | null;

export interface SheetView {
  name: string;
  rows: CellValue[][];
}

/** 시트 한 개에 보여 줄 최대 행 수(너무 큰 표로 화면이 멈추지 않게). */
export const MAX_SHEET_ROWS = 2000;

/** XLSX의 시트별 값(날짜는 YYYY-MM-DD 글자로). 서식·그림·차트는 보이지 않는다. */
export async function xlsxSheets(bytes: ArrayBuffer): Promise<SheetView[]> {
  const { default: readXlsxFile } = await import('read-excel-file/browser');
  const sheets = await readXlsxFile(bytes);
  return sheets.map(({ sheet, data }) => ({
    name: sheet,
    rows: data.slice(0, MAX_SHEET_ROWS).map((row) =>
      row.map((cell): CellValue => {
        if (cell instanceof Date) return cell.toISOString().slice(0, 10);
        if (typeof cell === 'string' || typeof cell === 'number' || typeof cell === 'boolean')
          return cell;
        return null;
      }),
    ),
  }));
}
