// 문서 한 건 내보내기(D-75): Markdown(.md), Markdown과 그림·첨부를 묶은 ZIP, PDF(브라우저 인쇄).
// 서버가 내보내기 권한(첨부 내려받기와 같은 규칙)을 확인하고 기록을 남긴 뒤 저장된 리비전을 준다. 변환은 편집기와
// 같은 스키마·Markdown 변환기(@tiptap/markdown)로 하고 두 번째 문서 모델을 두지 않는다. @참조는 이 사람이 볼 수 있는
// 대상만 제목·링크로 바꾸고, 그림·첨부는 파일 내용 API로 받아(파일마다 내려받기 권한 확인) 받을 수 있는 것만 담는다.
import { Editor } from '@tiptap/core';
import type { JSONContent } from '@tiptap/core';

import type { DocNode, ReferenceKind, ResolvedReferenceView } from '$lib/api/client';
import { api, fileContentUrl } from '$lib/api/client';
import { referenceLabel } from '$lib/features/references/resolver';
import { m } from '$lib/paraglide/messages.js';

import { canonicalJson } from './drafts';
import { schemaExtensions } from './editor-schema';

export type ExportFormat = 'markdown' | 'zip' | 'pdf';

// 내보낸 파일 안의 알림·대체 글자는 내보낸 사람의 화면 언어를 따른다(D-81). 본문(사용자 내용)은 그대로다.

/** 볼 수 없는 @참조에 쓰는 글자(이름·제목을 드러내지 않는다). */
export function hiddenReferenceLabel(): string {
  return m.references_hidden();
}

const FILE_URL = /^\/api\/files\/([0-9a-f-]{36})\/content(?:\?(.*))?$/;

/** 본문의 첨부 주소 하나(파일 id와 판). 판이 없으면 지금 판. */
interface FileRef {
  id: string;
  version: number | undefined;
}

function fileRef(url: unknown): FileRef | undefined {
  if (typeof url !== 'string') return undefined;
  const match = FILE_URL.exec(url);
  if (match === null) return undefined;
  const id = match[1];
  if (id === undefined) return undefined;
  const raw = new URLSearchParams(match[2] ?? '').get('version');
  const version = raw === null ? undefined : Number.parseInt(raw, 10);
  return { id, version: version !== undefined && Number.isFinite(version) ? version : undefined };
}

const fileKey = (f: FileRef): string => `${f.id}@${String(f.version ?? 'current')}`;

/** 저장된 본문(서버 JSON)을 편집기 JSON으로 옮긴다. 모양이 맞지 않는 값은 버리지 않고 글자로 둔다. */
export function toJson(node: unknown): JSONContent {
  if (typeof node !== 'object' || node === null) return { type: 'text', text: String(node) };
  const o: Record<string, unknown> = { ...node };
  const out: JSONContent = {};
  if (typeof o.type === 'string') out.type = o.type;
  if (typeof o.text === 'string') out.text = o.text;
  if (typeof o.attrs === 'object' && o.attrs !== null) out.attrs = { ...o.attrs };
  if (Array.isArray(o.marks)) {
    // 표시는 종류 이름이 있어야 한다. 없는 것은 버린다(편집기도 읽지 못한다).
    out.marks = o.marks.flatMap((m) => {
      const mark = toJson(m);
      if (typeof mark.type !== 'string') return [];
      return [
        mark.attrs === undefined ? { type: mark.type } : { type: mark.type, attrs: mark.attrs },
      ];
    });
  }
  if (Array.isArray(o.content)) out.content = o.content.map((c) => toJson(c));
  return out;
}

function walk(node: JSONContent, visit: (n: JSONContent) => void): void {
  visit(node);
  for (const child of node.content ?? []) walk(child, visit);
}

/** 바꾼 사본(원본은 그대로). `replace`가 배열을 돌려주면 그 노드를 그것들로 바꾼다. */
function mapNodes(
  node: JSONContent,
  replace: (n: JSONContent) => JSONContent | JSONContent[],
): JSONContent {
  const content = node.content?.flatMap((child) => {
    const next = replace(mapNodes(child, replace));
    return Array.isArray(next) ? next : [next];
  });
  return content === undefined ? { ...node } : { ...node, content };
}

export interface PreparedExport {
  title: string;
  revision: number;
  /** 변환할 본문(참조·첨부 주소를 바꾼 사본). */
  body: JSONContent;
  /** ZIP에 담을 파일(상대 경로 → 내용). */
  files: Map<string, Blob>;
  /** 사용자에게 알릴 변환 결과·제한. */
  notes: string[];
}

export interface PrepareOptions {
  format: ExportFormat;
  signal?: AbortSignal;
  /** 참조 대상으로 가는 앱 경로(볼 수 없으면 null). */
  hrefFor: (target: ResolvedReferenceView) => string | null;
  /** 이 앱의 원점(링크를 절대 주소로). */
  origin: string;
}

function safeName(name: string): string {
  const cleaned = name
    .replace(/[\\/:*?"<>|\p{Cc}]/gu, '_')
    .replace(/^\.+/, '')
    .trim()
    .slice(0, 80)
    // UTF-16 길이 한도는 유지하되, 끝에서 잘린 서로게이트가 URI 인코딩을 깨뜨리지 않게 한다.
    .replace(/[\uD800-\uDBFF]$/u, '');
  return cleaned === '' ? m.documents_export_name_fallback() : cleaned;
}

/**
 * ZIP 안 파일 이름과 Markdown에서 가리킬 상대 주소. uuidv7 앞부분은 시각이라 겹칠 수 있어 뒤(무작위) 부분과 판을
 * 붙인다. 주소는 경로 조각을 퍼센트 인코딩해 공백·괄호가 든 이름도 링크가 깨지지 않게 한다.
 */
export function zipPath(
  f: { id: string; version: number | undefined },
  name: string,
): { entry: string; href: string } {
  const tag = `${f.id.slice(-8)}${f.version === undefined ? '' : `-v${String(f.version)}`}`;
  const file = `${tag}-${safeName(name)}`;
  const href = encodeURIComponent(file).replace(
    /[()]/g,
    (c) => `%${c.charCodeAt(0).toString(16).toUpperCase()}`,
  );
  return { entry: `files/${file}`, href: `files/${href}` };
}

export function exportFileName(title: string, format: ExportFormat): string {
  return `${safeName(title)}.${format === 'zip' ? 'zip' : format === 'pdf' ? 'pdf' : 'md'}`;
}

async function resolveAll(
  refs: { kind: ReferenceKind; id: string }[],
): Promise<Map<string, ResolvedReferenceView>> {
  const out = new Map<string, ResolvedReferenceView>();
  for (let start = 0; start < refs.length; start += 100) {
    const part = refs.slice(start, start + 100);
    const resolved = await api.resolveReferences(part);
    resolved.forEach((r) => out.set(`${r.kind}:${r.id}`, r));
  }
  return out;
}

const KINDS: readonly ReferenceKind[] = ['item', 'document', 'task', 'user'];
const asKind = (v: unknown): ReferenceKind | undefined => KINDS.find((k) => k === v);

/**
 * 서버에서 받은 리비전 본문을 내보낼 모양으로 바꾼다. @참조는 볼 수 있는 대상만 제목(과 링크), 볼 수 없으면
 * {@link hiddenReferenceLabel}. 첨부: ZIP은 받을 수 있는 파일만 `files/` 아래 상대 경로로 담고 나머지는 알림, Markdown·PDF는
 * 앱 주소(로그인 필요)로 둔다.
 */
export async function prepareExport(
  title: string,
  revision: number,
  stored: DocNode,
  options: PrepareOptions,
): Promise<PreparedExport> {
  options.signal?.throwIfAborted();
  const notes: string[] = [];
  const doc = toJson(stored);

  const refs = new Map<string, { kind: ReferenceKind; id: string }>();
  const fileRefs = new Map<string, FileRef>();
  walk(doc, (n) => {
    if (n.type === 'reference') {
      const kind = asKind(n.attrs?.kind);
      const id: unknown = n.attrs?.id;
      if (kind !== undefined && typeof id === 'string') refs.set(`${kind}:${id}`, { kind, id });
    }
    if (n.type === 'image') {
      const f = fileRef(n.attrs?.src);
      if (f !== undefined) fileRefs.set(fileKey(f), f);
    }
    for (const mark of n.marks ?? []) {
      if (mark.type !== 'link') continue;
      const f = fileRef(mark.attrs?.href);
      if (f !== undefined) fileRefs.set(fileKey(f), f);
    }
  });

  const resolved =
    refs.size === 0
      ? new Map<string, ResolvedReferenceView>()
      : await resolveAll([...refs.values()]);
  let hidden = 0;

  // ZIP: 받을 수 있는 파일만 담는다. PDF: 받을 수 있는 그림만 그 주소로 두고 나머지는 안내로 바꾼다.
  const files = new Map<string, Blob>();
  const local = new Map<string, string>();
  const missing: string[] = [];
  if (options.format === 'zip' || options.format === 'pdf') {
    for (const [key, f] of fileRefs) {
      options.signal?.throwIfAborted();
      let name = f.id;
      try {
        name = (await api.file(f.id)).name;
      } catch {
        // 이름을 볼 수 없으면 id로 둔다(아래 내용 요청도 대부분 실패한다).
      }
      try {
        options.signal?.throwIfAborted();
        const response = await fetch(fileContentUrl(f.id, f.version), {
          credentials: 'same-origin',
          signal: options.signal ?? null,
        });
        if (!response.ok) throw new Error(String(response.status));
        if (options.format === 'zip') {
          const { entry, href } = zipPath(f, name);
          files.set(entry, await response.blob());
          local.set(key, href);
        } else {
          await response.body?.cancel();
          local.set(key, `${options.origin}${fileContentUrl(f.id, f.version)}`);
        }
      } catch {
        options.signal?.throwIfAborted();
        missing.push(name === f.id ? m.documents_export_unnamed_attachment() : name);
      }
    }
    if (missing.length > 0) {
      notes.push(
        m.documents_export_missing_files({ count: missing.length, names: missing.join(', ') }),
      );
    }
  } else if (fileRefs.size > 0) {
    notes.push(m.documents_export_links_note());
  }

  const fileUrl = (raw: unknown): string | undefined => {
    const f = fileRef(raw);
    if (f === undefined) return undefined;
    if (options.format !== 'markdown') return local.get(fileKey(f));
    return `${options.origin}${fileContentUrl(f.id, f.version)}`;
  };

  const body = mapNodes(doc, (n) => {
    if (n.type === 'reference') {
      const kind = asKind(n.attrs?.kind);
      const id: unknown = n.attrs?.id;
      const r =
        kind !== undefined && typeof id === 'string' ? resolved.get(`${kind}:${id}`) : undefined;
      if (!r?.visible) {
        hidden += 1;
        return { type: 'text', text: hiddenReferenceLabel() };
      }
      const href = options.hrefFor(r);
      const text = referenceLabel(r);
      return href === null
        ? { type: 'text', text }
        : {
            type: 'text',
            text,
            marks: [{ type: 'link', attrs: { href: `${options.origin}${href}` } }],
          };
    }
    if (n.type === 'image' && fileRef(n.attrs?.src) !== undefined) {
      const src = fileUrl(n.attrs?.src);
      if (src === undefined) {
        const alt =
          typeof n.attrs?.alt === 'string' && n.attrs.alt !== ''
            ? n.attrs.alt
            : m.documents_export_image_fallback();
        return {
          type: 'paragraph',
          content: [{ type: 'text', text: m.documents_export_image_missing({ alt }) }],
        };
      }
      return { ...n, attrs: { ...n.attrs, src } };
    }
    if (n.marks !== undefined) {
      const marks = n.marks.flatMap((mark) => {
        if (mark.type !== 'link' || fileRef(mark.attrs?.href) === undefined) return [mark];
        const href = fileUrl(mark.attrs?.href);
        return href === undefined ? [] : [{ ...mark, attrs: { ...mark.attrs, href } }];
      });
      return { ...n, marks };
    }
    return n;
  });
  if (hidden > 0) {
    notes.push(m.documents_export_hidden_refs({ count: hidden, label: hiddenReferenceLabel() }));
  }
  options.signal?.throwIfAborted();
  return { title, revision, body, files, notes };
}

function blockLabel(type: string | undefined): string {
  switch (type) {
    case 'paragraph':
      return m.documents_block_paragraph();
    case 'heading':
      return m.documents_block_heading();
    case 'table':
      return m.documents_block_table();
    case 'codeBlock':
      return m.documents_block_code();
    case 'blockMath':
      return m.documents_block_math();
    case 'image':
      return m.documents_block_image();
    case 'taskList':
      return m.documents_block_checklist();
    case 'bulletList':
      return m.documents_block_bullet_list();
    case 'orderedList':
      return m.documents_block_ordered_list();
    case 'blockquote':
      return m.documents_block_quote();
    case 'horizontalRule':
      return m.documents_block_rule();
    default:
      return type ?? m.documents_block_other();
  }
}

/** 편집기와 같은 스키마의 화면 없는 편집기(변환에만 쓴다). */
function converter(): Editor {
  return new Editor({ extensions: schemaExtensions(), content: '' });
}

export interface MarkdownResult {
  markdown: string;
  /** Markdown으로 그대로 옮기지 못한 블록(몇 번째, 무엇). */
  lossy: string[];
}

/** 본문을 Markdown으로. 최상위 블록마다 다시 읽어 보아 그대로 돌아오지 않는 블록을 알린다(조용히 버리지 않는다). */
export function toMarkdown(prepared: PreparedExport): MarkdownResult {
  const editor = converter();
  try {
    const manager = editor.markdown;
    if (manager === undefined) throw new Error('markdown extension missing');
    const normalize = (json: JSONContent): string =>
      canonicalJson(editor.schema.nodeFromJSON(json).toJSON());
    const lossy: string[] = [];
    const blocks = prepared.body.content ?? [];
    blocks.forEach((block, i) => {
      const single: JSONContent = { type: 'doc', content: [block] };
      let same: boolean;
      try {
        const back = manager.parse(manager.serialize(single));
        same = normalize(back) === normalize(single);
      } catch {
        same = false;
      }
      if (!same) {
        lossy.push(
          m.documents_export_lossy_block({ position: i + 1, label: blockLabel(block.type) }),
        );
      }
    });
    const body = manager.serialize(prepared.body);
    const lines = [`# ${prepared.title}`, ''];
    const notes = [...prepared.notes];
    if (lossy.length > 0) {
      notes.push(m.documents_export_lossy_note({ blocks: lossy.join(', ') }));
    }
    if (notes.length > 0) {
      lines.push('<!--', m.documents_export_notes_header({ revision: prepared.revision }));
      for (const note of notes) lines.push(`- ${note.replaceAll('--', '—')}`);
      lines.push('-->', '');
    }
    lines.push(body.trimEnd(), '');
    return { markdown: lines.join('\n'), lossy };
  } finally {
    editor.destroy();
  }
}

/** Markdown과 담을 수 있는 파일을 ZIP 하나로(상대 경로). */
export async function toZip(prepared: PreparedExport, markdown: string): Promise<Blob> {
  const { default: JSZip } = await import('jszip');
  const zip = new JSZip();
  zip.file(exportFileName(prepared.title, 'markdown'), markdown);
  for (const [path, blob] of prepared.files) zip.file(path, blob);
  return zip.generateAsync({ type: 'blob' });
}

/** 인쇄용 HTML 본문(편집기와 같은 스키마로 만든 HTML, 수식은 KaTeX로 그림). */
export async function printBodyHtml(prepared: PreparedExport): Promise<string> {
  const editor = converter();
  try {
    editor.commands.setContent(prepared.body);
    const html = editor.getHTML();
    const { default: katex } = await import('katex');
    const dom = new DOMParser().parseFromString(`<body>${html}</body>`, 'text/html');
    for (const el of dom.querySelectorAll('[data-type="inline-math"], [data-type="block-math"]')) {
      const latex = el.getAttribute('data-latex') ?? '';
      el.innerHTML = katex.renderToString(latex, {
        displayMode: el.getAttribute('data-type') === 'block-math',
        throwOnError: false,
        output: 'html',
      });
    }
    return dom.body.innerHTML;
  } finally {
    editor.destroy();
  }
}

/** 화면 이름 등에 쓰는 HTML 글자 이스케이프. */
export function escapeHtml(text: string): string {
  return text
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}

/** 브라우저가 받는 파일로 저장한다. */
export function saveBlob(blob: Blob, name: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => {
    URL.revokeObjectURL(url);
  }, 30_000);
}
