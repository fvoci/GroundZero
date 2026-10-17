// Office 미리보기(D-05, D-71 브라우저 우선). DOCX는 docx-preview(Apache-2.0)로 쪽 단위 HTML을 만들어 스크립트를 돌리지
// 못하는 iframe(sandbox)에 넣고, XLSX는 read-excel-file(MIT)로 시트별 값 표를 만든다. 둘 다 필요할 때만 불러온다.
// PPTX는 @aiden0z/pptx-renderer(D-74)를 같은 출처가 아닌 격리 iframe(`pptx-frame.html`) 안에서 돌려 그린다.

export type OfficeKind = 'docx' | 'xlsx' | 'pptx';

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
  if (
    type === 'application/vnd.openxmlformats-officedocument.presentationml.presentation' ||
    ext === 'pptx'
  )
    return 'pptx';
  return undefined;
}

/** PPTX를 그리는 프레임 스크립트(D-74). `vite.pptx-frame.config.ts`가 한 파일로 만든다. */
export const PPTX_FRAME_SCRIPT_URL = '/pptx-frame.js';

/**
 * 프레임 스크립트를 HTML의 <script> 안에 그대로 넣어도 되는지. 닫는 태그나 주석 시작이 있으면 넣지 않는다(고쳐 쓰면
 * 정규식 안 등에서 뜻이 바뀔 수 있다). 빌드된 스크립트에는 없고, 생기면 미리보기가 안전하게 실패한다.
 */
export function inlineSafe(code: string): boolean {
  return !/<\/script|<!--/i.test(code);
}

/**
 * PPTX 격리 프레임의 srcdoc(D-74). 프레임 스크립트를 이 원점에서 받아 nonce와 함께 문서 안에 넣는다. 프레임은
 * `sandbox="allow-scripts"`(출처 없음)로 열려 앱의 쿠키·저장소·화면에 닿지 못하고, CSP가 그 스크립트 말고는 어떤
 * 스크립트·요청도 막는다. 그림·글꼴은 파일 안의 것(blob:·data:)만.
 */
export async function pptxSrcdoc(): Promise<string> {
  const response = await fetch(PPTX_FRAME_SCRIPT_URL, { credentials: 'omit' });
  if (!response.ok) throw new Error(String(response.status));
  const code = await response.text();
  if (!inlineSafe(code)) throw new Error('pptx frame script cannot be inlined');
  const nonce = crypto.randomUUID().replaceAll('-', '');
  const csp = `default-src 'none'; script-src 'nonce-${nonce}'; img-src blob: data:; style-src 'unsafe-inline'; font-src blob: data:`;
  return `<!doctype html><html lang="ko"><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${csp}"></head><body style="margin:0;background:#e5e7eb"><div id="slides"></div><script nonce="${nonce}">${code}</script></body></html>`;
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
