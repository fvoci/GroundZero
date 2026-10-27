// 항목 기록의 입력 칸 ↔ 보낼 값, 표시용 요약. 칸 정의는 서버(`GET /api/record-kinds`)에서 받는다.
import type { RecordFieldValue, RecordInput, RecordKindView, RecordView } from '$lib/api/client';
import { m } from '$lib/paraglide/messages.js';
import { isWebUrl } from '$lib/url';
import { object, strings, stringArray } from '../modules/draft-values';

import { recordChoiceLabel, recordFieldLabel } from './labels';

export interface FileLinkForm {
  file_id: string;
  /** 빈 값이면 지금 버전. */
  version: string;
  role: string;
}

export interface RecordForm {
  title: string;
  occurred_on: string;
  body: string;
  /** 글자·수·날짜·고른 값·사람·강좌·태그(쉼표로 여럿) 칸. */
  values: Record<string, string>;
  /** 사람 여럿 칸. */
  people: Record<string, string[]>;
  files: FileLinkForm[];
  /**
   * 수 칸 가운데 브라우저가 수로 읽지 못한 글자가 든 칸("1e" 등). 그런 칸의 값은 빈 글자로 오므로, 일부러 비운 칸과
   * 구분하려고 따로 든다.
   */
  unreadable: Record<string, boolean>;
}

export function emptyForm(): RecordForm {
  return {
    title: '',
    occurred_on: '',
    body: '',
    values: {},
    people: {},
    files: [],
    unreadable: {},
  };
}

/** Restore all editor values, including incomplete numbers and attachment references. */
export function parseRecordForm(raw: unknown): RecordForm | undefined {
  if (
    !object(raw) ||
    typeof raw.title !== 'string' ||
    typeof raw.occurred_on !== 'string' ||
    typeof raw.body !== 'string' ||
    !strings(raw.values) ||
    !object(raw.people) ||
    !object(raw.unreadable) ||
    !Array.isArray(raw.files)
  )
    return undefined;
  const people: Record<string, string[]> = {};
  for (const [key, value] of Object.entries(raw.people)) {
    if (!stringArray(value)) return undefined;
    people[key] = value;
  }
  const unreadable: Record<string, boolean> = {};
  for (const [key, value] of Object.entries(raw.unreadable)) {
    if (typeof value !== 'boolean') return undefined;
    unreadable[key] = value;
  }
  const files: FileLinkForm[] = [];
  for (const file of raw.files) {
    if (
      !object(file) ||
      typeof file.file_id !== 'string' ||
      typeof file.version !== 'string' ||
      typeof file.role !== 'string'
    )
      return undefined;
    files.push({ file_id: file.file_id, version: file.version, role: file.role });
  }
  return {
    title: raw.title,
    occurred_on: raw.occurred_on,
    body: raw.body,
    values: raw.values,
    people,
    unreadable,
    files,
  };
}

/** 기록 → 입력 칸. */
export function formOf(def: RecordKindView, r: RecordView): RecordForm {
  const values: Record<string, string> = {};
  const people: Record<string, string[]> = {};
  for (const f of def.fields) {
    const v = r.fields[f.key];
    if (v === undefined) continue;
    if (f.type === 'people') people[f.key] = Array.isArray(v) ? v : [];
    else if (f.type === 'tags') values[f.key] = Array.isArray(v) ? v.join(', ') : String(v);
    else values[f.key] = Array.isArray(v) ? v.join(', ') : String(v);
  }
  return {
    title: r.title,
    occurred_on: r.occurred_on ?? '',
    body: r.body,
    values,
    people,
    files: r.files
      .filter((f) => f.file_id !== null)
      .map((f) => ({ file_id: f.file_id ?? '', version: String(f.version), role: f.role })),
    unreadable: {},
  };
}

/**
 * 입력 칸 → 보낼 값. 빈 칸은 뺀다. 수가 아니면 그 칸 이름을 오류로 돌려준다. 수로 읽지 못한 칸은 빈 칸으로 보내지 않고
 * 저장을 막는다(서버는 칸을 통째로 바꾸므로, 빼고 보내면 저장된 값이 지워진다).
 */
export function toInput(
  def: RecordKindView,
  form: RecordForm,
): { input: RecordInput } | { error: string } {
  const fields: Record<string, RecordFieldValue> = {};
  for (const f of def.fields) {
    if (f.type === 'people') {
      const list = form.people[f.key] ?? [];
      if (list.length > 0) fields[f.key] = list;
      continue;
    }
    if (f.type === 'number' && form.unreadable[f.key] === true)
      return { error: m.records_number_invalid({ label: recordFieldLabel(def, f) }) };
    const raw = (form.values[f.key] ?? '').trim();
    if (raw === '') continue;
    if (f.type === 'number') {
      const n = Number(raw);
      if (!Number.isFinite(n) || n < 0)
        return { error: m.records_number_invalid({ label: recordFieldLabel(def, f) }) };
      fields[f.key] = n;
    } else if (f.type === 'tags') {
      const tags = [
        ...new Set(
          raw
            .split(',')
            .map((t) => t.trim())
            .filter((t) => t !== ''),
        ),
      ];
      if (tags.length > 0) fields[f.key] = tags;
    } else {
      fields[f.key] = raw;
    }
  }
  return {
    input: {
      title: form.title.trim(),
      occurred_on: def.date_label === null || form.occurred_on === '' ? null : form.occurred_on,
      body: form.body,
      fields,
      files: form.files
        .filter((f) => f.file_id !== '')
        .map((f) => ({
          file_id: f.file_id,
          version: f.version === '' ? null : Number(f.version),
          role: f.role,
        })),
    },
  };
}

export interface SummaryLine {
  /** 칸 키. 표시 이름은 언어마다 달라지므로 칸과 줄을 맞출 때는 이것을 쓴다. */
  key: string;
  label: string;
  text: string;
  /** 주소 칸이면 링크로 보인다. */
  href?: string;
}

/**
 * 표시용 줄. 고른 값은 이름으로, 사람은 표시 이름으로(모르면 "알 수 없는 사람"), 강좌는 제목으로, 회차는 "번호. 제목"으로
 * (지워졌거나 볼 수 없으면 그렇게 적는다).
 */
export function summary(
  def: RecordKindView,
  r: RecordView,
  names: ReadonlyMap<string, string>,
  courses: ReadonlyMap<string, string>,
  episodes: ReadonlyMap<string, string> = new Map(),
): SummaryLine[] {
  const lines: SummaryLine[] = [];
  const person = (id: string): string => names.get(id) ?? m.records_unknown_person();
  for (const f of def.fields) {
    const v = r.fields[f.key];
    if (v === undefined) continue;
    let text: string;
    if (f.type === 'choice') {
      const c = f.choices.find((x) => x.value === v);
      text = c === undefined ? String(v) : recordChoiceLabel(def, f, c);
    } else if (f.type === 'person' && typeof v === 'string') text = person(v);
    else if (f.type === 'people' && Array.isArray(v)) text = v.map(person).join(', ');
    else if (f.type === 'course' && typeof v === 'string')
      text = courses.get(v) ?? m.records_hidden_course();
    else if (f.type === 'episode' && typeof v === 'string')
      text = episodes.get(v) ?? m.records_hidden_episode();
    else text = Array.isArray(v) ? v.join(', ') : String(v);
    const line: SummaryLine = { key: f.key, label: recordFieldLabel(def, f), text };
    if (f.type === 'url' && typeof v === 'string' && isWebUrl(v)) line.href = v;
    lines.push(line);
  }
  return lines;
}
