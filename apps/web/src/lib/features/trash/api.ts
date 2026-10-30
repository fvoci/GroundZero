// 시스템 자원 읽기·명령. 응답 타입은 OpenAPI에서 생성하고 런타임에서도 확인한다.
import type { components } from '@fvoci/api-client';
import { expectOk } from '$lib/shared/api';
import {
  bool,
  int,
  none,
  optInt,
  optStr,
  parseList,
  rec,
  str,
  type Json,
} from '$lib/shared/api/parse';

export type TrashEntryView = components['schemas']['TrashEntry'];
type RetentionSettings = components['schemas']['RetentionSettings'];
export interface TrashView extends RetentionSettings {
  entries: TrashEntryView[];
}

function parseEntry(value: Json): TrashEntryView {
  const e = rec(value, 'trash batch');
  const kind = str(e, 'kind');
  if (
    kind !== 'item' &&
    kind !== 'document' &&
    kind !== 'task' &&
    kind !== 'event' &&
    kind !== 'file' &&
    kind !== 'module'
  )
    throw new TypeError(`unknown trash kind ${kind}`);
  const expiry = str(e, 'expiry');
  if (expiry !== 'active' && expiry !== 'expired' && expiry !== 'kept_for_references')
    throw new TypeError(`unknown trash expiry ${expiry}`);
  return {
    id: str(e, 'id'),
    kind,
    root_id: str(e, 'root_id'),
    title: str(e, 'title'),
    member_count: int(e, 'member_count'),
    deleted_by: str(e, 'deleted_by'),
    deleted_by_name: optStr(e, 'deleted_by_name'),
    deleted_at: str(e, 'deleted_at'),
    expires_at: str(e, 'expires_at'),
    expiry,
    can_purge: bool(e, 'can_purge'),
  };
}

function parseRetention(value: Json): RetentionSettings {
  const o = rec(value, 'retention');
  return {
    retention_days: int(o, 'retention_days'),
    version_retention_days: optInt(o, 'version_retention_days'),
  };
}
const batch = (value: Json): string => str(rec(value, 'trashed'), 'batch_id');

export const systemApi = {
  batches: (workspaceId: string | null): Promise<TrashEntryView[]> =>
    expectOk(
      'GET',
      workspaceId === null
        ? '/api/trash-batches'
        : `/api/trash-batches?workspace_id=${encodeURIComponent(workspaceId)}`,
      (value) => parseList(value, parseEntry),
    ),
  retention: (workspaceId: string): Promise<RetentionSettings> =>
    expectOk('GET', `/api/workspaces/${encodeURIComponent(workspaceId)}/retention`, parseRetention),
  trashDocument: (id: string) =>
    expectOk('DELETE', `/api/documents/${encodeURIComponent(id)}`, batch),
  trashItem: (id: string) => expectOk('DELETE', `/api/items/${encodeURIComponent(id)}`, batch),
  trashModule: (id: string) => expectOk('DELETE', `/api/modules/${encodeURIComponent(id)}`, batch),
  restoreTrash: (id: string) =>
    expectOk('POST', `/api/trash-batches/${encodeURIComponent(id)}/restore`, none),
  purgeTrash: (id: string) =>
    expectOk('DELETE', `/api/trash-batches/${encodeURIComponent(id)}`, none),
  purgeExpiredTrash: (workspaceId: string | null) =>
    expectOk(
      'POST',
      workspaceId === null
        ? '/api/trash-batches/purge-expired'
        : `/api/trash-batches/purge-expired?workspace_id=${encodeURIComponent(workspaceId)}`,
      (v) => int(rec(v, 'purged'), 'purged'),
    ),
  setTrashRetention: (workspaceId: string, days: number) =>
    expectOk('PUT', `/api/workspaces/${encodeURIComponent(workspaceId)}/retention/trash`, none, {
      days,
    }),
  setVersionRetention: (workspaceId: string, days: number | null) =>
    expectOk(
      'PUT',
      `/api/workspaces/${encodeURIComponent(workspaceId)}/retention/file-versions`,
      none,
      { days },
    ),
};

// 화면 조합. 목록과 설정의 권한·트랜잭션은 각 자원 읽기가 맡는다.
export async function readTrash(workspaceId: string | null): Promise<TrashView> {
  const [entries, retention] = await Promise.all([
    systemApi.batches(workspaceId),
    workspaceId === null
      ? Promise.resolve({ retention_days: 30, version_retention_days: null })
      : systemApi.retention(workspaceId),
  ]);
  return { ...retention, entries };
}
