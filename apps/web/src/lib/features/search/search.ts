// 검색 결과를 여는 경로와 종류 이름.
import type { SearchKind, SearchResultView } from '$lib/api/client';
import { m } from '$lib/paraglide/messages.js';
import { calendarPath, itemPath, itemTaskPath, pathFor } from '$lib/router.svelte';

export const SEARCH_KINDS: SearchKind[] = ['document', 'task', 'item', 'event', 'file', 'record'];

/** 종류 이름. 표시 언어가 바뀔 수 있어 부를 때 계산한다. */
export function searchKindLabel(kind: SearchKind): string {
  switch (kind) {
    case 'document':
      return m.search_kind_document();
    case 'task':
      return m.search_kind_task();
    case 'item':
      return m.search_kind_item();
    case 'event':
      return m.search_kind_event();
    case 'file':
      return m.search_kind_file();
    case 'record':
      return m.search_kind_record();
  }
}

/** 결과가 놓인 곳의 경로. 워크스페이스 밖 문서(개인 노트, 받은 공유)는 단독 경로로 연다. */
export function searchResultPath(
  r: SearchResultView,
  memberOf: (ws: string) => boolean,
): string | undefined {
  if (r.kind === 'document') return pathFor(r, memberOf);
  if (r.kind === 'file' && r.document_id !== null) {
    return pathFor(
      { id: r.document_id, workspace_id: r.workspace_id, item_id: r.item_id },
      memberOf,
    );
  }
  if (r.workspace_id === null) return undefined;
  if (r.kind === 'item') return itemPath(r.workspace_id, r.id);
  if (r.kind === 'record')
    return r.item_id === null ? undefined : itemPath(r.workspace_id, r.item_id);
  if (r.kind === 'file')
    return r.item_id === null ? undefined : itemPath(r.workspace_id, r.item_id);
  if (r.kind === 'task') {
    return r.item_id === null ? undefined : itemTaskPath(r.workspace_id, r.item_id, r.id);
  }
  return r.item_id === null ? calendarPath(r.workspace_id) : itemPath(r.workspace_id, r.item_id);
}

// 본문의 참조 노드는 색인 글에 `[reference kind="…" id="…"]`로 들어 있다(색인 내용은 그대로 둔다).
const REFERENCE_MARK = /\[reference kind="[a-z_]+" id="[0-9a-f-]+"\]/g;

/** 결과 요약에서 참조 표기를 id 대신 짧은 표시(↗참조)로 바꾼다. */
export function readableSnippet(text: string): string {
  return text.replace(REFERENCE_MARK, `↗${m.search_snippet_reference()}`);
}
