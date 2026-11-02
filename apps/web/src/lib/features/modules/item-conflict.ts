// 항목 정보·프로필 충돌(D-94 개정, D-104): 공통 비교 부품(FieldConflict)에 넘기는 항목 칸 정의와 비교 상태.
// 409의 `current`에 편집하는 칸이 모두 있으므로 서버 값은 그 응답만으로 만든다(후속 GET 없음).
import type { ModuleKind } from '@fvoci/api-client';

import { ITEM_STATUSES, type ItemProfileView, type ItemStatus } from '$lib/api/client';
import {
  fieldDifferences,
  initialFieldChoices,
  type ConflictField,
  type FieldSide,
} from '$lib/features/common/field-conflict';
import { trimLikeServer } from '$lib/features/events/conflict';
import { m } from '$lib/paraglide/messages.js';

import { object, strings } from './draft-values';
import { detailFields, itemStatusLabel, periodText } from './profile';

const shown = (value: string): string =>
  trimLikeServer(value) === '' ? m.modules_item_conflict_empty() : value;

/** 항목 정보 폼의 입력(제목·설명·공개 범위). */
export interface ItemInfoInput {
  title: string;
  description: string;
  shared: boolean;
}

/**
 * 항목 정보의 비교 칸. 공개 범위는 소유 권한자만 바꿀 수 있어 그때만 고른다(다른 사람이 "내 입력"을 고르면 보낼 수 없는
 * 값이 된다). 제목·설명은 서버가 앞뒤 공백을 지워 저장하므로 지운 값으로 비교한다.
 */
export function itemInfoFields(owner: boolean): ConflictField<ItemInfoInput>[] {
  const fields: ConflictField<ItemInfoInput>[] = [
    {
      key: 'title',
      label: m.modules_title(),
      same: (a, b) => trimLikeServer(a.title) === trimLikeServer(b.title),
      text: (value) => shown(value.title),
      copy: (target, source) => {
        target.title = source.title;
      },
    },
    {
      key: 'description',
      label: m.modules_description(),
      same: (a, b) => trimLikeServer(a.description) === trimLikeServer(b.description),
      text: (value) => shown(value.description),
      copy: (target, source) => {
        target.description = source.description;
      },
    },
  ];
  if (!owner) return fields;
  return [
    ...fields,
    {
      key: 'shared',
      label: m.modules_share_with_members(),
      same: (a, b) => a.shared === b.shared,
      text: (value) =>
        value.shared ? m.modules_item_conflict_shared() : m.modules_item_conflict_not_shared(),
      copy: (target, source) => {
        target.shared = source.shared;
      },
    },
  ];
}

/** 항목 프로필 폼의 입력. 키워드는 입력 글 그대로(쉼표로 나눔), 종류별 칸은 `values`. */
export interface ItemProfileInput {
  status: ItemStatus;
  leader: string;
  goal: string;
  scope: string;
  starts_on: string;
  ends_on: string;
  keywords: string;
  values: Record<string, string>;
}

/** 서버 프로필 → 폼 입력(편집 열기와 같은 모양). */
export function profileInputOf(p: ItemProfileView, kind: ModuleKind): ItemProfileInput {
  return {
    status: p.status,
    leader: p.leader?.user_id ?? '',
    goal: p.goal,
    scope: p.scope,
    starts_on: p.starts_on ?? '',
    ends_on: p.ends_on ?? '',
    keywords: (p.details.keywords ?? []).join(', '),
    values: Object.fromEntries(detailFields(kind).map((f) => [f.key, p.details[f.key] ?? ''])),
  };
}

/** 서버가 저장하는 키워드 집합(앞뒤 공백 지움·빈 것과 중복 뺌). 순서는 보지 않는다(D-94 집합 단위). */
function keywordSet(text: string): string {
  const list = text
    .split(',')
    .map(trimLikeServer)
    .filter((k) => k !== '');
  return JSON.stringify([...new Set(list)].sort());
}

function textField(key: 'goal' | 'scope', label: string): ConflictField<ItemProfileInput> {
  return {
    key,
    label,
    same: (a, b) => trimLikeServer(a[key]) === trimLikeServer(b[key]),
    text: (value) => shown(value[key]),
    copy: (target, source) => {
      target[key] = source[key];
    },
  };
}

/**
 * 항목 프로필의 비교 칸: 상태, 리더, 기간(시작·끝 한 묶음), 목표, 범위, 연구의 키워드(한 집합), 종류별 칸.
 * `names`는 리더 id → 이름(409의 서버 리더, 고를 수 있는 사람, 편집을 연 때의 리더).
 */
export function itemProfileFields(
  kind: ModuleKind,
  names: ReadonlyMap<string, string>,
  leaderLabel: string,
): ConflictField<ItemProfileInput>[] {
  const fields: ConflictField<ItemProfileInput>[] = [
    {
      key: 'status',
      label: m.modules_status(),
      same: (a, b) => a.status === b.status,
      text: (value) => itemStatusLabel(value.status),
      copy: (target, source) => {
        target.status = source.status;
      },
    },
    {
      key: 'leader',
      label: leaderLabel,
      same: (a, b) => a.leader === b.leader,
      text: (value) =>
        value.leader === ''
          ? m.modules_none()
          : (names.get(value.leader) ?? m.modules_leader_unknown()),
      copy: (target, source) => {
        target.leader = source.leader;
      },
    },
    {
      key: 'period',
      label: m.modules_period(),
      same: (a, b) => a.starts_on === b.starts_on && a.ends_on === b.ends_on,
      text: (value) =>
        value.starts_on === '' && value.ends_on === ''
          ? m.modules_item_conflict_empty()
          : periodText(value.starts_on, value.ends_on),
      copy: (target, source) => {
        target.starts_on = source.starts_on;
        target.ends_on = source.ends_on;
      },
    },
    textField('goal', m.modules_goal()),
    textField('scope', m.modules_scope()),
  ];
  if (kind === 'research')
    fields.push({
      key: 'keywords',
      label: m.modules_keywords(),
      same: (a, b) => keywordSet(a.keywords) === keywordSet(b.keywords),
      text: (value) => shown(value.keywords),
      copy: (target, source) => {
        target.keywords = source.keywords;
      },
    });
  for (const f of detailFields(kind))
    fields.push({
      key: `detail:${f.key}`,
      label: f.label,
      same: (a, b) =>
        trimLikeServer(a.values[f.key] ?? '') === trimLikeServer(b.values[f.key] ?? ''),
      text: (value) => shown(value.values[f.key] ?? ''),
      copy: (target, source) => {
        target.values = { ...target.values, [f.key]: source.values[f.key] ?? '' };
      },
    });
  return fields;
}

/**
 * 열린(또는 닫았지만 아직 저장하지 않은) 비교. `base`는 편집 기준값, `mine`은 이번 저장의 입력, `server`는 409의
 * 최신값과 그 판(`version`). `original*`은 첫 409 앞의 입력과 기준이며 재충돌에도 바뀌지 않는다. `target`은 항목 id다
 * (다른 항목의 초안에서 비교를 되살리지 않게).
 */
export interface ItemComparison<T> {
  target: string;
  base: T;
  mine: T;
  server: T;
  version: number;
  baseVersion: number;
  original: T;
  originalBase: T;
  originalVersion: number;
  choices: Record<string, FieldSide>;
  open: boolean;
  repeated: boolean;
}

/** 첫 409면 새 비교, 재충돌이면 처음 입력을 지키며 다시 고르게 한다(자동 재전송 없음). */
export function nextComparison<T>(
  old: ItemComparison<T> | undefined,
  fields: readonly ConflictField<T>[],
  next: { target: string; base: T; mine: T; server: T; version: number; baseVersion: number },
): ItemComparison<T> {
  const kept = old?.target === next.target ? old : undefined;
  return {
    ...next,
    original: kept?.original ?? next.mine,
    originalBase: kept?.originalBase ?? next.base,
    originalVersion: kept?.originalVersion ?? next.baseVersion,
    choices: initialFieldChoices(fieldDifferences(fields, next.base, next.mine, next.server)),
    open: true,
    repeated: kept !== undefined,
  };
}

function sideChoices(value: unknown): value is Record<string, FieldSide> {
  return (
    strings(value) && Object.values(value).every((side) => side === 'mine' || side === 'server')
  );
}

/** 브라우저 저장소의 비교를 믿지 않고 모양을 확인한다. 다른 항목의 비교는 버린다. */
export function parseComparison<T>(
  raw: unknown,
  target: string,
  parse: (value: unknown) => T | undefined,
): ItemComparison<T> | undefined {
  if (
    !object(raw) ||
    raw.target !== target ||
    typeof raw.version !== 'number' ||
    typeof raw.baseVersion !== 'number' ||
    typeof raw.originalVersion !== 'number' ||
    typeof raw.open !== 'boolean' ||
    typeof raw.repeated !== 'boolean' ||
    !sideChoices(raw.choices)
  )
    return undefined;
  const base = parse(raw.base),
    mine = parse(raw.mine),
    server = parse(raw.server),
    original = parse(raw.original),
    originalBase = parse(raw.originalBase);
  if (
    base === undefined ||
    mine === undefined ||
    server === undefined ||
    original === undefined ||
    originalBase === undefined
  )
    return undefined;
  return {
    target,
    base,
    mine,
    server,
    version: raw.version,
    baseVersion: raw.baseVersion,
    original,
    originalBase,
    originalVersion: raw.originalVersion,
    choices: raw.choices,
    open: raw.open,
    repeated: raw.repeated,
  };
}

export function parseInfoInput(raw: unknown): ItemInfoInput | undefined {
  return object(raw) &&
    typeof raw.title === 'string' &&
    typeof raw.description === 'string' &&
    typeof raw.shared === 'boolean'
    ? { title: raw.title, description: raw.description, shared: raw.shared }
    : undefined;
}

export function parseProfileInput(raw: unknown): ItemProfileInput | undefined {
  if (!object(raw)) return undefined;
  const status = ITEM_STATUSES.find((candidate) => candidate === raw.status);
  if (
    status === undefined ||
    typeof raw.leader !== 'string' ||
    typeof raw.goal !== 'string' ||
    typeof raw.scope !== 'string' ||
    typeof raw.starts_on !== 'string' ||
    typeof raw.ends_on !== 'string' ||
    typeof raw.keywords !== 'string' ||
    !strings(raw.values)
  )
    return undefined;
  return {
    status,
    leader: raw.leader,
    goal: raw.goal,
    scope: raw.scope,
    starts_on: raw.starts_on,
    ends_on: raw.ends_on,
    keywords: raw.keywords,
    values: raw.values,
  };
}
