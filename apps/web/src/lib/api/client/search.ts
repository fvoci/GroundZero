import { expectOk } from '$lib/shared/api';
import { optStr, parseList, rec, str, type Json } from '$lib/shared/api/parse';
import type { StatusCategory } from '@fvoci/api-client';

export type SearchKind = 'document' | 'task' | 'item' | 'event' | 'file' | 'record';

/** 검색 필터(D-39). 상태·담당자를 주면 업무만 나온다. 날짜는 YYYY-MM-DD(끝 포함). */
export interface SearchFilters {
  kind?: SearchKind;
  moduleId?: string;
  /** 업무 상태 범주(D-116). 상태 이름이 아니라 범주로 거르며, 주면 업무만 나온다. */
  statusCategory?: StatusCategory;
  /** `me` 또는 사용자 id. */
  assignee?: string;
  from?: string;
  to?: string;
  /** 날짜의 하루를 정하는 IANA 시간대(설정의 시간대). 경계는 서버가 계산한다(D-101). */
  tz?: string;
  /** 작성자: `me` 또는 사용자 id(그 사람이 만든 것만). */
  author?: string;
  /** 제목에서만 찾기. */
  titleOnly?: boolean;
}

export interface SearchResultView {
  kind: SearchKind;
  id: string;
  title: string;
  reference: string | null;
  workspace_id: string | null;
  item_id: string | null;
  document_id: string | null;
  snippet: string | null;
}

function parseSearchResult(value: Json): SearchResultView {
  const o = rec(value, 'search result');
  const kind = str(o, 'kind');
  if (
    kind !== 'document' &&
    kind !== 'task' &&
    kind !== 'item' &&
    kind !== 'event' &&
    kind !== 'file' &&
    kind !== 'record'
  )
    throw new TypeError(`unknown search kind ${kind}`);
  return {
    kind,
    id: str(o, 'id'),
    title: str(o, 'title'),
    reference: optStr(o, 'reference'),
    workspace_id: optStr(o, 'workspace_id'),
    item_id: optStr(o, 'item_id'),
    document_id: optStr(o, 'document_id'),
    snippet: optStr(o, 'snippet'),
  };
}

export const searchApi = {
  /** 통합 검색. 볼 수 있는 것만, 최대 20건. */
  /** 검색은 현재 조직 범위다(D-111). `workspaceId`는 그 안의 좁힘이지 조직을 대신하지 않는다. */
  search: (
    organizationId: string,
    q: string,
    workspaceId: string | null,
    filters: SearchFilters = {},
  ) => {
    const params = new URLSearchParams({ organization_id: organizationId, q });
    if (workspaceId !== null) params.set('workspace_id', workspaceId);
    if (filters.kind !== undefined) params.set('kinds', filters.kind);
    if (filters.moduleId !== undefined) params.set('module_id', filters.moduleId);
    if (filters.statusCategory !== undefined) params.set('status_category', filters.statusCategory);
    if (filters.assignee !== undefined) params.set('assignee', filters.assignee);
    if (filters.from !== undefined) params.set('from', filters.from);
    if (filters.to !== undefined) params.set('to', filters.to);
    if (filters.tz !== undefined) params.set('tz', filters.tz);
    if (filters.author !== undefined) params.set('author', filters.author);
    if (filters.titleOnly === true) params.set('title_only', 'true');
    return expectOk('GET', `/api/search?${params.toString()}`, (v) =>
      parseList(v, parseSearchResult),
    );
  },
};
