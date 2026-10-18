// 붙여넣기·끌어 놓기로 들어오는 내용을 서버의 문서 규칙에 맞춘다(crates/server/src/features/documents.rs의
// check_image_node·check_link_mark와 같은 규칙). 웹 페이지에서 복사한 바깥 이미지(추적 이미지)나 허용하지 않는
// 링크가 그대로 들어오면 일반 편집은 저장이 거절되고, 실시간 편집은 방이 닫힌다. 그런 것은 들어올 때 뺀다:
// 바깥 이미지는 빼고, 허용하지 않는 링크는 글자만 남긴다.
import { Extension } from '@tiptap/core';
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

/** 링크 주소: 스킴이 있으면 http·https·mailto만, 없으면 다른 호스트로 가는 `//`만 막는다. 서버 규칙과 같다. */
export function isAllowedHref(href: string): boolean {
  const value = href.trim();
  const scheme = /^([a-z][a-z0-9+.-]*):/i.exec(value)?.[1]?.toLowerCase();
  if (scheme !== undefined) return scheme === 'http' || scheme === 'https' || scheme === 'mailto';
  return !value.startsWith('//');
}

function cleanFragment(fragment: Fragment): Fragment {
  const nodes: PmNode[] = [];
  let changed = false;
  for (let i = 0; i < fragment.childCount; i += 1) {
    const node = fragment.child(i);
    if (node.type.name === 'image') {
      const src: unknown = node.attrs.src;
      if (typeof src !== 'string' || !isAttachmentUrl(src)) {
        changed = true;
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
        changed = true;
      }
    } else if (node.content.size > 0) {
      const content = cleanFragment(node.content);
      if (content !== node.content) {
        // 빼고 남은 내용이 이 노드에 맞지 않으면(이미지만 있던 목록 항목 등) 필요한 빈 칸을 채워 다시 만든다.
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

/** 붙여넣기·끌어 놓기 조각에서 서버가 받지 않는 이미지·링크를 뺀다. */
export function cleanSlice(slice: Slice): Slice {
  return new Slice(cleanFragment(slice.content), slice.openStart, slice.openEnd);
}

export const PasteGuard = Extension.create({
  name: 'pasteGuard',
  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: new PluginKey('pasteGuard'),
        props: { transformPasted: (slice) => cleanSlice(slice) },
      }),
    ];
  },
});
