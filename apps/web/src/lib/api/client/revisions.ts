import { expectOk } from '$lib/shared/api';
import { int, optInt, optStr, parseList, rec, str, type Json } from '$lib/shared/api/parse';
import { parseDocNode, type DocNode } from './documents';

// --- 리비전 -------------------------------------------------------------

export interface RevisionSummaryView {
  revision: number;
  title: string;
  author_name: string | null;
  created_at: string;
  restored_from: number | null;
  /**
   * 저장한 사람이 고친 사람 대신 남긴 리비전(실시간 편집, D-95)이면 실제로 고친 사람. 보통 리비전은 null, 기록이 없으면 빈
   * 목록. 이름을 볼 수 없는 사람은 null.
   */
  edited_by_names: (string | null)[] | null;
}

export interface RevisionView extends RevisionSummaryView {
  body: DocNode;
  schema_version: number;
}

/** 이름 목록(항목은 이름이나 null). 없으면 null. */
function optNameList(o: Record<string, unknown>, key: string): (string | null)[] | null {
  const v = o[key];
  if (v === null || v === undefined) return null;
  if (!Array.isArray(v)) throw new TypeError(`expected array or null at ${key}`);
  return v.map((name: unknown) => {
    if (name === null || typeof name === 'string') return name;
    throw new TypeError(`expected string or null in ${key}`);
  });
}

export function parseRevisionSummary(value: Json): RevisionSummaryView {
  const o = rec(value, 'revision');
  return {
    revision: int(o, 'revision'),
    title: str(o, 'title'),
    author_name: optStr(o, 'author_name'),
    created_at: str(o, 'created_at'),
    restored_from: optInt(o, 'restored_from'),
    edited_by_names: optNameList(o, 'edited_by_names'),
  };
}

export function parseRevision(value: Json): RevisionView {
  const o = rec(value, 'revision');
  return {
    ...parseRevisionSummary(o),
    body: parseDocNode(o.body),
    schema_version: int(o, 'schema_version'),
  };
}

export const revisionsApi = {
  revisions: (id: string, before: number | null) =>
    expectOk(
      'GET',
      `/api/documents/${encodeURIComponent(id)}/revisions${before === null ? '' : `?before=${String(before)}`}`,
      (v) => parseList(v, parseRevisionSummary),
    ),
  revision: (id: string, revision: number) =>
    expectOk(
      'GET',
      `/api/documents/${encodeURIComponent(id)}/revisions/${String(revision)}`,
      parseRevision,
    ),
  /** 저장된 리비전 하나를 내보낸다(D-75). 서버가 내보내기 권한을 확인하고 기록을 남긴 뒤 그 리비전을 준다. */
  exportRevision: (
    id: string,
    revision: number,
    format: 'markdown' | 'zip' | 'pdf',
    signal?: AbortSignal,
  ) =>
    expectOk(
      'POST',
      `/api/documents/${encodeURIComponent(id)}/revisions/${String(revision)}/export`,
      parseRevision,
      { format },
      signal,
    ),
};
