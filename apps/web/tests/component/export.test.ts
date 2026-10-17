import { afterEach, describe, expect, it, vi } from 'vitest';

import type { DocNode, ResolvedReferenceView } from '$lib/api/client';
import {
  HIDDEN_REFERENCE,
  exportFileName,
  prepareExport,
  printBodyHtml,
  toMarkdown,
  zipPath,
} from '$lib/features/documents/export';

const visibleId = '0198f1f2-0000-7000-8000-000000000001';
const hiddenId = '0198f1f2-0000-7000-8000-000000000002';
const okFile = '0198f1f2-0000-7000-8000-0000000000aa';
const deniedFile = '0198f1f2-0000-7000-8000-0000000000bb';

function isRef(v: unknown): v is { kind: string; id: string } {
  return (
    typeof v === 'object' &&
    v !== null &&
    'id' in v &&
    typeof v.id === 'string' &&
    'kind' in v &&
    typeof v.kind === 'string'
  );
}

/** 서버 대역: 참조 해석(하나만 볼 수 있음), 파일 정보는 없음(404), 파일 내용은 하나만 내려받기 허용. */
function stubServer(): void {
  vi.stubGlobal('fetch', (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
    if (url.endsWith('/api/references/resolve')) {
      const raw = init?.body;
      const body: unknown = typeof raw === 'string' ? JSON.parse(raw) : null;
      const refs =
        typeof body === 'object' && body !== null && 'refs' in body && Array.isArray(body.refs)
          ? body.refs.filter(isRef)
          : [];
      const out: ResolvedReferenceView[] = refs.map((r) =>
        r.id === visibleId
          ? {
              kind: 'item',
              id: r.id,
              visible: true,
              title: '조사 항목',
              reference: 'RES-3',
              workspace_id: 'ws',
              item_id: null,
            }
          : {
              kind: 'item',
              id: r.id,
              visible: false,
              title: null,
              reference: null,
              workspace_id: null,
              item_id: null,
            },
      );
      return Promise.resolve(new Response(JSON.stringify(out), { status: 200 }));
    }
    if (url.includes(`/api/files/${okFile}/content`)) {
      return Promise.resolve(new Response(new Uint8Array([1, 2, 3]), { status: 200 }));
    }
    return Promise.resolve(
      new Response('{"error":"no"}', { status: url.includes('/content') ? 403 : 404 }),
    );
  });
}

afterEach(() => {
  vi.unstubAllGlobals();
});

const text = (t: string, marks?: object[]): object =>
  marks ? { type: 'text', text: t, marks } : { type: 'text', text: t };
const para = (...content: object[]): object => ({ type: 'paragraph', content });

function body(): DocNode {
  return {
    type: 'doc',
    content: [
      { type: 'heading', attrs: { level: 2 }, content: [text('결과')] },
      para(
        text('참고 '),
        { type: 'reference', attrs: { kind: 'item', id: visibleId } },
        text(' 와 '),
        { type: 'reference', attrs: { kind: 'item', id: hiddenId } },
        text(' 및 '),
        text('원문', [{ type: 'link', attrs: { href: 'https://example.org/paper' } }]),
      ),
      {
        type: 'taskList',
        content: [
          { type: 'taskItem', attrs: { checked: true }, content: [para(text('수집'))] },
          { type: 'taskItem', attrs: { checked: false }, content: [para(text('분석'))] },
        ],
      },
      { type: 'codeBlock', attrs: { language: 'rust' }, content: [text('fn main() {}')] },
      {
        type: 'table',
        content: [
          {
            type: 'tableRow',
            content: [
              { type: 'tableHeader', content: [para(text('항목'))] },
              { type: 'tableHeader', content: [para(text('값'))] },
            ],
          },
          {
            type: 'tableRow',
            content: [
              { type: 'tableCell', content: [para(text('정확도'))] },
              { type: 'tableCell', content: [para(text('0.91'))] },
            ],
          },
        ],
      },
      { type: 'image', attrs: { src: `/api/files/${okFile}/content?version=1`, alt: '그래프' } },
      { type: 'image', attrs: { src: `/api/files/${deniedFile}/content`, alt: '비공개 그림' } },
    ],
  };
}

const options = (format: 'markdown' | 'zip' | 'pdf') => ({
  format,
  origin: 'https://fvoci.example.test',
  hrefFor: (r: ResolvedReferenceView) => (r.visible ? `/w/ws/items/${r.id}` : null),
});

describe('document export (D-75)', () => {
  it('keeps headings, links, checklists, code and tables, and only names visible references', async () => {
    stubServer();
    const prepared = await prepareExport('실험 노트', 4, body(), options('markdown'));
    const { markdown, lossy } = toMarkdown(prepared);
    expect(markdown.startsWith('# 실험 노트\n')).toBe(true);
    expect(markdown).toContain('## 결과');
    expect(markdown).toContain('[RES-3 조사 항목](https://fvoci.example.test/w/ws/items/');
    expect(markdown).toContain(HIDDEN_REFERENCE);
    expect(markdown).not.toContain(hiddenId);
    expect(markdown).toContain('[원문](https://example.org/paper)');
    expect(markdown).toMatch(/- \[x\] 수집/);
    expect(markdown).toMatch(/- \[ \] 분석/);
    expect(markdown).toContain('```rust');
    expect(markdown).toMatch(/\| *항목 *\| *값 *\|/);
    // Markdown 한 파일은 첨부를 앱 주소로 둔다.
    expect(markdown).toContain(`https://fvoci.example.test/api/files/${okFile}/content?version=1`);
    expect(prepared.notes.some((n) => n.includes('ZIP'))).toBe(true);
    expect(prepared.notes.some((n) => n.includes('@참조 1개'))).toBe(true);
    expect(lossy).toEqual([]);
  });

  it('reports blocks Markdown cannot hold instead of dropping them silently', async () => {
    stubServer();
    const merged: DocNode = {
      type: 'doc',
      content: [
        {
          type: 'table',
          content: [
            {
              type: 'tableRow',
              content: [
                { type: 'tableHeader', attrs: { colspan: 2 }, content: [para(text('합친 칸'))] },
              ],
            },
            {
              type: 'tableRow',
              content: [
                { type: 'tableCell', content: [para(text('a'))] },
                { type: 'tableCell', content: [para(text('b'))] },
              ],
            },
          ],
        },
      ],
    };
    const prepared = await prepareExport('표', 1, merged, options('markdown'));
    const { markdown, lossy } = toMarkdown(prepared);
    expect(lossy).toEqual(['1번째 블록(표)']);
    expect(markdown).toContain('fvoci 내보내기 알림');
    expect(markdown).toContain('합친 칸');
  });

  it('bundles only files this person may download, with relative paths', async () => {
    stubServer();
    const prepared = await prepareExport('실험 노트', 4, body(), options('zip'));
    expect([...prepared.files.keys()]).toEqual([`files/${okFile.slice(-8)}-v1-${okFile}`]);
    const { markdown } = toMarkdown(prepared);
    expect(markdown).toContain(`](files/${okFile.slice(-8)}-v1-${okFile})`);
    expect(markdown).not.toContain(deniedFile);
    expect(markdown).toContain('(비공개 그림: 내려받을 수 없어 넣지 못함)');
    expect(prepared.notes.some((n) => n.includes('내려받을 수 없는 그림·첨부 1개'))).toBe(true);
  });

  it('renders math with KaTeX for the print view and names files safely', async () => {
    stubServer();
    const math: DocNode = {
      type: 'doc',
      content: [{ type: 'blockMath', attrs: { latex: 'E = mc^2' } }, para(text('한글 문단'))],
    };
    const prepared = await prepareExport('수식', 2, math, options('pdf'));
    const html = await printBodyHtml(prepared);
    expect(html).toContain('katex');
    expect(html).toContain('한글 문단');
    expect(exportFileName('a/b:c', 'zip')).toBe('a_b_c.zip');
    expect(exportFileName('  ', 'markdown')).toBe('문서.md');
  });
});

describe('export paths and PDF images (review)', () => {
  it('percent-encodes ZIP links so names with spaces and brackets stay linked', () => {
    const p = zipPath({ id: okFile, version: 2 }, '회의 자료 (최종).png');
    expect(p.entry).toBe(`files/${okFile.slice(-8)}-v2-회의 자료 (최종).png`);
    expect(decodeURIComponent(p.href)).toBe(p.entry);
    expect(p.href).not.toMatch(/[ ()]/);
  });

  it('replaces images the exporter cannot download in the PDF view and says so', async () => {
    stubServer();
    const prepared = await prepareExport('실험 노트', 4, body(), options('pdf'));
    const html = await printBodyHtml(prepared);
    expect(html).toContain(`https://fvoci.example.test/api/files/${okFile}/content?version=1`);
    expect(html).not.toContain(deniedFile);
    expect(html).toContain('(비공개 그림: 내려받을 수 없어 넣지 못함)');
    expect(prepared.notes.some((n) => n.includes('내려받을 수 없는 그림·첨부 1개'))).toBe(true);
  });
});
