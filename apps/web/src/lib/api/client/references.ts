import { expectOk } from '$lib/shared/api';
import { bool, optStr, parseList, rec, str, type Json } from '$lib/shared/api/parse';

// --- @ 참조 -------------------------------------------------------------

export type ReferenceKind = 'item' | 'document' | 'task' | 'user';

function referenceKind(value: string): ReferenceKind {
  if (value === 'item' || value === 'document' || value === 'task' || value === 'user')
    return value;
  throw new TypeError(`unknown reference kind ${value}`);
}

export interface ReferenceCandidateView {
  kind: ReferenceKind;
  id: string;
  title: string;
  reference: string | null;
  workspace_id: string | null;
  workspace_name: string | null;
}

/** 해석 결과. 볼 수 없거나 없는 대상은 visible=false이고 나머지는 null이다. */
export interface ResolvedReferenceView {
  kind: ReferenceKind;
  id: string;
  visible: boolean;
  title: string | null;
  reference: string | null;
  workspace_id: string | null;
  item_id: string | null;
}

export interface BacklinkView {
  id: string;
  reference: string | null;
  title: string;
  workspace_id: string | null;
  item_id: string | null;
}

export function parseCandidate(value: Json): ReferenceCandidateView {
  const o = rec(value, 'reference candidate');
  return {
    kind: referenceKind(str(o, 'kind')),
    id: str(o, 'id'),
    title: str(o, 'title'),
    reference: optStr(o, 'reference'),
    workspace_id: optStr(o, 'workspace_id'),
    workspace_name: optStr(o, 'workspace_name'),
  };
}

export function parseResolved(value: Json): ResolvedReferenceView {
  const o = rec(value, 'resolved reference');
  return {
    kind: referenceKind(str(o, 'kind')),
    id: str(o, 'id'),
    visible: bool(o, 'visible'),
    title: optStr(o, 'title'),
    reference: optStr(o, 'reference'),
    workspace_id: optStr(o, 'workspace_id'),
    item_id: optStr(o, 'item_id'),
  };
}

export function parseBacklink(value: Json): BacklinkView {
  const o = rec(value, 'backlink');
  return {
    id: str(o, 'id'),
    reference: optStr(o, 'reference'),
    title: str(o, 'title'),
    workspace_id: optStr(o, 'workspace_id'),
    item_id: optStr(o, 'item_id'),
  };
}

export const referencesApi = {
  taskBacklinks: (id: string) =>
    expectOk('GET', `/api/tasks/${encodeURIComponent(id)}/backlinks`, (v) =>
      parseList(v, parseBacklink),
    ),
  referenceCandidates: (q: string, workspaceId: string | null) => {
    const params = new URLSearchParams({ q });
    if (workspaceId !== null) params.set('workspace_id', workspaceId);
    return expectOk('GET', `/api/reference-candidates?${params.toString()}`, (v) =>
      parseList(v, parseCandidate),
    );
  },
  resolveReferences: (refs: { kind: ReferenceKind; id: string }[], signal?: AbortSignal) =>
    expectOk(
      'POST',
      '/api/references/resolve',
      (v) => parseList(v, parseResolved),
      { refs },
      signal,
    ),
  documentBacklinks: (id: string) =>
    expectOk('GET', `/api/documents/${encodeURIComponent(id)}/backlinks`, (v) =>
      parseList(v, parseBacklink),
    ),
  itemBacklinks: (id: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(id)}/backlinks`, (v) =>
      parseList(v, parseBacklink),
    ),
};
