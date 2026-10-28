import type { components } from '@fvoci/api-client';
import { bool, int, optInt, optStr, rec, str, type Json } from '$lib/shared/api/parse';
type Schemas = components['schemas'];
export type FileView = Required<Schemas['FileView']>;
export type FileVersionView = Required<Schemas['VersionView']>;
export type UploadView = Required<Schemas['UploadView']>;
export interface FileTarget {
  kind: 'document' | 'item';
  id: string;
}

export function parseFile(value: Json): FileView {
  const o = rec(value, 'file');
  return {
    id: str(o, 'id'),
    name: str(o, 'name'),
    document_id: optStr(o, 'document_id'),
    item_id: optStr(o, 'item_id'),
    current_version: int(o, 'current_version'),
    size: int(o, 'size'),
    content_type: str(o, 'content_type'),
    updated_at: str(o, 'updated_at'),
    can_write: bool(o, 'can_write'),
    can_download: bool(o, 'can_download'),
  };
}

export function parseVersion(value: Json): FileVersionView {
  const o = rec(value, 'file version');
  return {
    version: int(o, 'version'),
    size: int(o, 'size'),
    sha256: str(o, 'sha256'),
    content_type: str(o, 'content_type'),
    note: str(o, 'note'),
    uploaded_by: str(o, 'uploaded_by'),
    uploaded_by_name: optStr(o, 'uploaded_by_name'),
    created_at: str(o, 'created_at'),
  };
}

export function parseUpload(value: Json): UploadView {
  const o = rec(value, 'upload');
  const received = Array.isArray(o.received) ? o.received : [];
  return {
    id: str(o, 'id'),
    state: str(o, 'state'),
    name: str(o, 'name'),
    size: int(o, 'size'),
    chunk_bytes: int(o, 'chunk_bytes'),
    parts_total: int(o, 'parts_total'),
    received: received.filter((n): n is number => typeof n === 'number'),
    expires_at: str(o, 'expires_at'),
    file_id: optStr(o, 'file_id'),
    version: optInt(o, 'version'),
    failure: optStr(o, 'failure'),
  };
}
