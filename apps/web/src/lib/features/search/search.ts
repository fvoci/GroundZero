// 검색 결과를 여는 경로와 종류 이름.
import type { SearchKind, SearchResultView } from '$lib/api/client';
import { calendarPath, itemPath, itemTaskPath, pathFor } from '$lib/router.svelte';

export const SEARCH_KINDS: SearchKind[] = ['document', 'task', 'item', 'event', 'file', 'record'];

export const SEARCH_KIND_LABEL: Record<SearchKind, string> = {
  document: '문서',
  task: '업무',
  item: '항목',
  event: '일정',
  file: '파일',
  record: '기록',
};

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
