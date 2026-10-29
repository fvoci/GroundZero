import type { components } from '@fvoci/api-client';
import { bool, int, optStr, parseList, rec, str, type Json } from '$lib/shared/api/parse';

// --- 항목 기록 (D-33·D-34·D-36·D-38) -------------------------------------

export type RecordFieldType =
  | 'text'
  | 'long_text'
  | 'date'
  | 'number'
  | 'choice'
  | 'url'
  | 'tags'
  | 'person'
  | 'people'
  | 'course'
  /** 이 강좌의 회차(D-123 강의 질문·복습). */
  | 'episode';

const RECORD_FIELD_TYPES: readonly RecordFieldType[] = [
  'text',
  'long_text',
  'date',
  'number',
  'choice',
  'url',
  'tags',
  'person',
  'people',
  'course',
  'episode',
];

export interface ChoiceView {
  value: string;
  label: string;
}

export interface RecordFieldDef {
  key: string;
  label: string;
  type: RecordFieldType;
  choices: ChoiceView[];
}

export interface RecordKindView {
  kind: string;
  label: string;
  module_kind: string;
  date_label: string | null;
  fields: RecordFieldDef[];
  file_roles: ChoiceView[];
}

/** 칸 값: 글자·수·날짜·고른 값·사람 id·사람 id 목록·태그 목록. */
export type RecordFieldValue = string | number | string[];

export interface RecordFileView extends Required<components['schemas']['RecordFile']> {
  role: string;
  file_id: string | null;
  file_name: string;
  version: number;
  sha256: string;
  size: number;
  available: boolean;
}

export interface RecordView extends Omit<
  Required<components['schemas']['Record']>,
  'fields' | 'files'
> {
  id: string;
  kind: string;
  title: string;
  occurred_on: string | null;
  body: string;
  fields: Record<string, RecordFieldValue>;
  files: RecordFileView[];
  created_by: string;
  created_by_name: string | null;
  updated_at: string;
  version: number;
  can_edit: boolean;
  can_delete: boolean;
}

export interface RecordInput {
  title: string;
  occurred_on: string | null;
  body: string;
  fields: Record<string, RecordFieldValue>;
  files: { file_id: string; version: number | null; role: string }[];
}

function parseChoice(value: Json): ChoiceView {
  const o = rec(value, 'choice');
  return { value: str(o, 'value'), label: str(o, 'label') };
}

export function parseRecordKind(value: Json): RecordKindView {
  const o = rec(value, 'record kind');
  return {
    kind: str(o, 'kind'),
    label: str(o, 'label'),
    module_kind: str(o, 'module_kind'),
    date_label: optStr(o, 'date_label'),
    fields: parseList(o.fields, (f): RecordFieldDef => {
      const r = rec(f, 'record field');
      const type = RECORD_FIELD_TYPES.find((t) => t === r.type);
      if (type === undefined) throw new TypeError('unknown record field type');
      return {
        key: str(r, 'key'),
        label: str(r, 'label'),
        type,
        choices: parseList(r.choices, parseChoice),
      };
    }),
    file_roles: parseList(o.file_roles, parseChoice),
  };
}

function recordFieldValue(value: unknown): RecordFieldValue | undefined {
  if (typeof value === 'string' || typeof value === 'number') return value;
  if (Array.isArray(value) && value.every((v) => typeof v === 'string')) return value;
  return undefined;
}

export function parseRecord(value: Json): RecordView {
  const o = rec(value, 'record');
  const raw = rec(o.fields ?? {}, 'record fields');
  const fields: Record<string, RecordFieldValue> = {};
  for (const [key, v] of Object.entries(raw)) {
    const parsed = recordFieldValue(v);
    if (parsed !== undefined) fields[key] = parsed;
  }
  return {
    id: str(o, 'id'),
    kind: str(o, 'kind'),
    title: str(o, 'title'),
    occurred_on: optStr(o, 'occurred_on'),
    body: str(o, 'body'),
    fields,
    files: parseList(o.files, (f): RecordFileView => {
      const r = rec(f, 'record file');
      return {
        role: str(r, 'role'),
        file_id: optStr(r, 'file_id'),
        file_name: str(r, 'file_name'),
        version: int(r, 'version'),
        sha256: str(r, 'sha256'),
        size: int(r, 'size'),
        available: bool(r, 'available'),
      };
    }),
    created_by: str(o, 'created_by'),
    created_by_name: optStr(o, 'created_by_name'),
    updated_at: str(o, 'updated_at'),
    version: int(o, 'version'),
    can_edit: bool(o, 'can_edit'),
    can_delete: bool(o, 'can_delete'),
  };
}

// --- Zotero 읽기 (D-34) ------------------------------------------------------

export interface ZoteroLibraryInput {
  api_key: string;
  library_type: 'user' | 'group';
  library_id: string;
}
