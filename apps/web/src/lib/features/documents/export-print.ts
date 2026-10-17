// PDF 내보내기(D-75): 저장된 리비전을 인쇄용 문서로 만들어 브라우저 인쇄(“PDF로 저장”)에 넘긴다. 새 서비스나 PDF
// 엔진을 두지 않는다. 앱은 글꼴을 배포하지 않고(수식은 KaTeX 배치만, D-68 KaTeX 글꼴 미포함) 사용자 기기의 글꼴을 쓰며,
// PDF에 글꼴을 넣는 것은 브라우저가 그 글꼴의 임베딩 허용 범위 안에서 한다. 인쇄 문서는 스크립트 없이(같은 출처
// iframe, sandbox: allow-same-origin allow-modals) 그림이 다 불러와진 뒤 인쇄한다.
import katexCss from '../../../styles/katex-layout.css?inline';

import type { PreparedExport } from './export';
import { escapeHtml, printBodyHtml } from './export';
import { m } from '$lib/paraglide/messages.js';
import { getLocale } from '$lib/paraglide/runtime.js';

const PRINT_CSS = `
@page { size: A4; margin: 18mm 16mm; }
html { -webkit-print-color-adjust: exact; print-color-adjust: exact; }
body { margin: 0; color: #111; font-size: 10.5pt; line-height: 1.65;
  font-family: system-ui, -apple-system, 'Apple SD Gothic Neo', 'Malgun Gothic', 'Noto Sans KR', 'Noto Sans CJK KR', sans-serif;
  word-break: keep-all; overflow-wrap: anywhere; }
h1.export-title { font-size: 20pt; margin: 0 0 4pt; }
p.export-meta { color: #555; font-size: 8.5pt; margin: 0 0 14pt; }
h1, h2, h3, h4 { break-after: avoid; page-break-after: avoid; }
table { border-collapse: collapse; width: 100%; margin: 6pt 0; }
th, td { border: 1px solid #9ca3af; padding: 3pt 6pt; vertical-align: top; }
th { background: #eef2f7; }
th > p, td > p { margin: 0; }
tr, img, pre, blockquote, .katex-display, li[data-type="taskItem"] { break-inside: avoid; page-break-inside: avoid; }
img { max-width: 100%; height: auto; }
pre { white-space: pre-wrap; word-break: break-all; background: #f5f7fa; border: 1px solid #e5e7eb;
  padding: 6pt 8pt; font-size: 9pt; font-family: ui-monospace, Menlo, Consolas, 'D2Coding', monospace; }
code { font-family: ui-monospace, Menlo, Consolas, 'D2Coding', monospace; font-size: 0.92em; }
blockquote { border-left: 3pt solid #cbd5e1; margin: 6pt 0; padding: 0 0 0 8pt; color: #374151; }
ul[data-type="taskList"] { list-style: none; padding-left: 0; }
li[data-type="taskItem"] { display: flex; gap: 6pt; align-items: flex-start; }
li[data-type="taskItem"] > label { flex: none; }
li[data-type="taskItem"] > div > p { margin: 0; }
a { color: #1d4ed8; }
`;

/** 인쇄 문서 전체(제목·리비전 표시, 본문). */
export async function printDocumentHtml(
  prepared: PreparedExport,
  savedAt: string,
): Promise<string> {
  const body = await printBodyHtml(prepared);
  const title = escapeHtml(prepared.title);
  return `<!doctype html><html lang="${getLocale()}"><head><meta charset="utf-8"><title>${title}</title><style>${katexCss}${PRINT_CSS}</style></head><body><h1 class="export-title">${title}</h1><p class="export-meta">${escapeHtml(m.documents_revision_label())} ${String(prepared.revision)} · ${escapeHtml(savedAt)}</p>${body}</body></html>`;
}

/** 인쇄 문서를 보이지 않는 iframe에 넣고 그림을 다 불러온 뒤 인쇄 창을 연다. */
export async function printToPdf(html: string): Promise<void> {
  const frame = document.createElement('iframe');
  frame.setAttribute('sandbox', 'allow-same-origin allow-modals');
  frame.setAttribute('aria-hidden', 'true');
  frame.style.position = 'fixed';
  frame.style.width = '0';
  frame.style.height = '0';
  frame.style.border = '0';
  frame.style.right = '0';
  frame.style.bottom = '0';
  const loaded = new Promise<void>((resolve) => {
    frame.addEventListener('load', () => {
      resolve();
    });
  });
  frame.srcdoc = html;
  document.body.append(frame);
  try {
    await loaded;
    const win = frame.contentWindow;
    const doc = frame.contentDocument;
    if (win === null || doc === null) throw new Error('print frame unavailable');
    await Promise.all(
      [...doc.images].map((img) =>
        img.complete ? Promise.resolve() : img.decode().catch(() => undefined),
      ),
    );
    win.focus();
    win.print();
  } finally {
    // 인쇄 창이 닫힌 뒤(대부분의 브라우저는 print가 끝날 때까지 기다린다) 정리한다.
    setTimeout(() => {
      frame.remove();
    }, 1000);
  }
}
