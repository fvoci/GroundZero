// 문서 내용 규칙(서버 crates/server/src/features/documents.rs의 check_image_node·check_link_mark와 같다).
// 이미지는 이 서비스의 첨부 주소만, 링크는 https·http·mailto나 문서 안 경로만 저장된다. 바깥 이미지(추적 이미지)나
// 허용하지 않는 링크가 그대로 들어가면 일반 편집은 저장이 거절되고 실시간 편집은 방이 닫힌다.
// 들어오는 곳마다 조용히 지우지 않고 알린다:
// - 붙여넣기·끌어 놓기: 바깥 http(s) 이미지는 같은 주소의 링크 글로 바꾸고, 링크로도 둘 수 없는 이미지(data: 등)는
//   빼고, 허용하지 않는 링크는 글자만 남긴 뒤 몇 개를 바꿨는지 알린다(`onAdjusted`).
// - `![설명](주소)` 입력: 첨부 주소면 이미지, 바깥 http(s) 주소면 링크로 넣고 알린다. 그 밖은 적은 글자 그대로 둔다.
// - Markdown 원문·초안 등 위를 거치지 않은 내용: 저장 전에 `contentIssues`로 찾아 저장하지 않고 원인과 고칠 방법을
//   보인다(초안은 그대로 남는다).
import type { JSONContent } from '@tiptap/core';
import { Extension, InputRule } from '@tiptap/core';
import { Fragment, Slice } from '@tiptap/pm/model';
import type { Node as PmNode } from '@tiptap/pm/model';
import { Plugin, PluginKey } from '@tiptap/pm/state';

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

/** 이 서비스의 첨부 내용 주소(`/api/files/{id}/content` + 없음·inline·version). 서버 is_file_url과 같다. */
export function isAttachmentUrl(url: string): boolean {
  const rest = url.startsWith('/api/files/') ? url.slice('/api/files/'.length) : undefined;
  if (rest === undefined) return false;
  const at = rest.indexOf('/content');
  if (at < 0) return false;
  const id = rest.slice(0, at);
  const query = rest.slice(at + '/content'.length);
  if (!UUID.test(id)) return false;
  if (query === '' || query === '?inline=true') return true;
  if (!query.startsWith('?version=')) return false;
  const v = query.slice('?version='.length).replace(/&inline=true$/, '');
  return /^[1-9][0-9]*$/.test(v) && Number(v) <= 2147483647;
}

/**
 * 링크 주소: 스킴이 있으면 http·https·mailto만, 없으면 다른 호스트로 가는 `//`를 막고 `/api/files/` 아래는 첨부
 * 표준형만. 제어 문자는 어디에도 두지 않는다. 서버 check_link_mark와 같다.
 */
export function isAllowedHref(href: string): boolean {
  const value = href.trim();
  for (const ch of value) {
    const code = ch.codePointAt(0) ?? 0;
    if (code < 0x20 || (code >= 0x7f && code <= 0x9f)) return false;
  }
  const scheme = /^([a-z][a-z0-9+.-]*):/i.exec(value)?.[1]?.toLowerCase();
  if (scheme !== undefined) return scheme === 'http' || scheme === 'https' || scheme === 'mailto';
  if (value.startsWith('//')) return false;
  return !value.toLowerCase().startsWith('/api/files/') || isAttachmentUrl(href);
}

/** 바깥 이미지 주소가 링크로 둘 수 있는 http(s) 주소인가. */
function isWebUrl(src: string): boolean {
  return /^https?:\/\//i.test(src.trim()) && isAllowedHref(src);
}

export interface ContentAdjustments {
  /** 바깥 http(s) 이미지를 링크 글로 바꾼 수. */
  imagesLinked: number;
  /** 링크로도 둘 수 없어 뺀 이미지 수(data: 등). */
  imagesRemoved: number;
  /** 링크를 풀고 글자만 남긴 수. */
  linksUnlinked: number;
}

export const noAdjustments = (): ContentAdjustments => ({
  imagesLinked: 0,
  imagesRemoved: 0,
  linksUnlinked: 0,
});

/** 바깥 이미지 대신 넣는 문단: 설명(없으면 주소)을 글로, 주소를 링크로. */
function imageAsLink(node: PmNode, src: string): PmNode | undefined {
  const { schema } = node.type;
  const paragraph = schema.nodes.paragraph;
  const link = schema.marks.link;
  if (paragraph === undefined || link === undefined) return undefined;
  const alt: unknown = node.attrs.alt;
  const label = typeof alt === 'string' && alt.trim() !== '' ? alt.trim() : src;
  return paragraph.create(null, schema.text(label, [link.create({ href: src })]));
}

function cleanFragment(fragment: Fragment, stats: ContentAdjustments): Fragment {
  const nodes: PmNode[] = [];
  let changed = false;
  for (let i = 0; i < fragment.childCount; i += 1) {
    const node = fragment.child(i);
    if (node.type.name === 'image') {
      const src: unknown = node.attrs.src;
      if (typeof src !== 'string' || !isAttachmentUrl(src)) {
        changed = true;
        const replacement =
          typeof src === 'string' && isWebUrl(src) ? imageAsLink(node, src) : undefined;
        if (replacement === undefined) {
          stats.imagesRemoved += 1;
        } else {
          stats.imagesLinked += 1;
          nodes.push(replacement);
        }
        continue;
      }
    }
    let next = node;
    if (node.isText) {
      const marks = node.marks.filter((mark) => {
        if (mark.type.name !== 'link') return true;
        const href: unknown = mark.attrs.href;
        return typeof href === 'string' && isAllowedHref(href);
      });
      if (marks.length !== node.marks.length) {
        next = node.mark(marks);
        stats.linksUnlinked += 1;
        changed = true;
      }
    } else if (node.content.size > 0) {
      const content = cleanFragment(node.content, stats);
      if (content !== node.content) {
        // 바꾼 내용이 이 노드에 맞지 않으면(이미지만 있던 목록 항목 등) 필요한 빈 칸을 채워 다시 만든다.
        const rebuilt = node.type.validContent(content)
          ? node.copy(content)
          : node.type.createAndFill(node.attrs, content, node.marks);
        changed = true;
        if (rebuilt === null) continue;
        next = rebuilt;
      }
    }
    nodes.push(next);
  }
  return changed ? Fragment.fromArray(nodes) : fragment;
}

/** 붙여넣기·끌어 놓기 조각을 문서 규칙에 맞춘다. 바꾼 수는 `stats`에 더한다. */
export function cleanSlice(slice: Slice, stats: ContentAdjustments = noAdjustments()): Slice {
  return new Slice(cleanFragment(slice.content, stats), slice.openStart, slice.openEnd);
}

export interface ContentIssues {
  /** 저장할 수 없는 이미지 주소. */
  images: string[];
  /** 저장할 수 없는 링크 주소. */
  links: string[];
}

/** 저장 전 확인: 문서 JSON에서 서버가 받지 않을 이미지·링크를 찾는다. */
export function contentIssues(doc: JSONContent): ContentIssues {
  const issues: ContentIssues = { images: [], links: [] };
  const walk = (node: JSONContent): void => {
    if (node.type === 'image') {
      const src: unknown = node.attrs?.src;
      if (typeof src !== 'string' || !isAttachmentUrl(src))
        issues.images.push(typeof src === 'string' ? src : '');
    }
    for (const mark of node.marks ?? []) {
      if (mark.type !== 'link') continue;
      const href: unknown = mark.attrs?.href;
      if (typeof href !== 'string' || !isAllowedHref(href))
        issues.links.push(typeof href === 'string' ? href : '');
    }
    for (const child of node.content ?? []) walk(child);
  };
  walk(doc);
  return issues;
}

/**
 * 붙인 HTML을 편집기가 읽기 전에 문서 규칙에 맞춘다(편집기의 읽기 규칙이 data: 이미지나 javascript: 링크를
 * 조용히 버리기 전에 세려고). 바깥 http(s) 이미지는 같은 주소의 링크로, 링크로도 둘 수 없는 이미지는 빼고,
 * 허용하지 않는 링크는 글자만 남긴다. 바꾼 수를 `stats`에 더한다.
 */
export function cleanPastedHtml(html: string, stats: ContentAdjustments): string {
  const doc = new DOMParser().parseFromString(html, 'text/html');
  for (const img of Array.from(doc.querySelectorAll('img'))) {
    const src = img.getAttribute('src') ?? '';
    if (isAttachmentUrl(src)) continue;
    if (isWebUrl(src)) {
      const a = doc.createElement('a');
      a.setAttribute('href', src);
      const alt = (img.getAttribute('alt') ?? '').trim();
      a.textContent = alt === '' ? src : alt;
      img.replaceWith(a);
      stats.imagesLinked += 1;
    } else {
      img.remove();
      stats.imagesRemoved += 1;
    }
  }
  for (const a of Array.from(doc.querySelectorAll('a[href]'))) {
    if (isAllowedHref(a.getAttribute('href') ?? '')) continue;
    a.replaceWith(...Array.from(a.childNodes));
    stats.linksUnlinked += 1;
  }
  return doc.body.innerHTML;
}

export interface PasteGuardOptions {
  /** 붙여넣기·끌어 놓기에서 바꾼 것이 있으면 부른다(화면이 사용자에게 알린다). */
  onAdjusted: (stats: ContentAdjustments) => void;
}

export const PasteGuard = Extension.create<PasteGuardOptions>({
  name: 'pasteGuard',
  addOptions() {
    return { onAdjusted: () => undefined };
  },
  addProseMirrorPlugins() {
    const { onAdjusted } = this.options;
    // HTML로 붙이면 읽기 전에 HTML에서 고치고(cleanPastedHtml) 센다. 그 밖(끌어 놓기 등)은 cleanSlice가 맡는다.
    let fromHtml = noAdjustments();
    return [
      new Plugin({
        key: new PluginKey('pasteGuard'),
        props: {
          transformPastedHTML: (html) => {
            fromHtml = noAdjustments();
            return cleanPastedHtml(html, fromHtml);
          },
          transformPasted: (slice) => {
            const stats = fromHtml;
            fromHtml = noAdjustments();
            const cleaned = cleanSlice(slice, stats);
            if (stats.imagesLinked + stats.imagesRemoved + stats.linksUnlinked > 0)
              onAdjusted(stats);
            return cleaned;
          },
        },
      }),
    ];
  },
});

/** Tiptap Image의 `![설명](주소 "제목")` 입력 꼴(같은 정규식). */
const IMAGE_INPUT = /(?:^|\s)(!\[(.+|:?)]\((\S+)(?:(?:\s+)["'](\S+)["'])?\))$/;

export interface SafeImageInputOptions {
  /** 바깥 이미지 주소를 링크로 넣었을 때. */
  onLinked: () => void;
}

/**
 * `![설명](주소)` 입력. 첨부 주소는 이미지로, 바깥 http(s) 주소는 링크 글로 넣고 알린다. 그 밖(javascript: 등)은
 * 적은 글자 그대로 둔다. 기본 Image 입력 규칙은 editor-schema에서 껐다(주소를 가리지 않으므로).
 */
export const SafeImageInput = Extension.create<SafeImageInputOptions>({
  name: 'safeImageInput',
  addOptions() {
    return { onLinked: () => undefined };
  },
  addInputRules() {
    const { onLinked } = this.options;
    return [
      new InputRule({
        find: IMAGE_INPUT,
        handler: ({ state, range, match }) => {
          const [whole, written, alt, src, title] = match;
          if (written === undefined || src === undefined) return null;
          const start = range.from + (whole.length - written.length);
          const { schema, tr } = state;
          if (isAttachmentUrl(src)) {
            const image = schema.nodes.image;
            if (image === undefined) return null;
            tr.replaceWith(
              start,
              range.to,
              image.create({ src, alt: alt ?? null, title: title ?? null }),
            );
            return undefined;
          }
          if (!isWebUrl(src)) return null;
          const link = schema.marks.link;
          if (link === undefined) return null;
          const label = alt !== undefined && alt.trim() !== '' ? alt.trim() : src;
          tr.replaceWith(start, range.to, schema.text(label, [link.create({ href: src })]));
          onLinked();
          return undefined;
        },
      }),
    ];
  },
});
