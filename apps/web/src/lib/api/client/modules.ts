import { request, expectOk, fail } from '$lib/shared/api';
import {
  access,
  bool,
  int,
  isRecord,
  optStr,
  parseList,
  rec,
  str,
  strings,
  type Json,
} from '$lib/shared/api/parse';
import type { Item, Module, ModuleKind } from '@fvoci/api-client';
import { none, visibility } from './common';

export const MODULE_KINDS: readonly ModuleKind[] = [
  'project',
  'research',
  'study',
  'course',
  'hobby',
] as const;

function moduleKind(value: string): ModuleKind {
  const kind = MODULE_KINDS.find((k) => k === value);
  if (kind === undefined) throw new TypeError(`unknown module kind ${value}`);
  return kind;
}

export function parseModule(value: Json): Module {
  const o = rec(value, 'module');
  return {
    id: str(o, 'id'),
    workspace_id: str(o, 'workspace_id'),
    kind: moduleKind(str(o, 'kind')),
    name: str(o, 'name'),
    key: str(o, 'key'),
    previous_keys: strings(o.previous_keys),
    archived: bool(o, 'archived'),
    icon: str(o, 'icon'),
    description: str(o, 'description'),
    access: access(str(o, 'access')),
  };
}

/** 항목 상태(D-33): 계획·진행·보류·완료·취소. */
export const ITEM_STATUSES = ['planned', 'active', 'on_hold', 'done', 'cancelled'] as const;

export type ItemStatus = (typeof ITEM_STATUSES)[number];

function itemStatus(value: string): ItemStatus {
  const found = ITEM_STATUSES.find((s) => s === value);
  if (found === undefined) throw new TypeError(`unknown item status ${value}`);
  return found;
}

/** 화면에서 쓰는 항목. 공개 범위·상태를 좁힌 것 외에는 계약의 Item과 같다. */
export type ItemView = Item & { visibility: 'workspace' | 'private'; status: ItemStatus };

export function parseItem(value: Json): ItemView {
  const o = rec(value, 'item');
  return {
    id: str(o, 'id'),
    module_id: str(o, 'module_id'),
    workspace_id: str(o, 'workspace_id'),
    reference: optStr(o, 'reference'),
    title: str(o, 'title'),
    description: str(o, 'description'),
    visibility: visibility(str(o, 'visibility')),
    access: access(str(o, 'access')),
    archived: bool(o, 'archived'),
    updated_at: str(o, 'updated_at'),
    status: itemStatus(str(o, 'status')),
    starts_on: optStr(o, 'starts_on'),
    ends_on: optStr(o, 'ends_on'),
    is_template: bool(o, 'is_template'),
    version: int(o, 'version'),
  };
}

/** 모듈 종류별 항목 정보(D-34 연구, D-36 스터디, D-38 취미). 종류에 없는 칸은 보내지 않는다. */
export interface ItemDetailsView {
  field?: string;
  keywords?: string[];
  question?: string;
  hypothesis?: string;
  model?: string;
  method?: string;
  sample?: string;
  level?: string;
  target?: string;
  activity_type?: string;
}

export interface PersonRefView {
  user_id: string;
  display_name: string;
}

export interface ItemProfileView {
  leader: PersonRefView | null;
  goal: string;
  scope: string;
  starts_on: string | null;
  ends_on: string | null;
  status: ItemStatus;
  details: ItemDetailsView;
  members: PersonRefView[];
  groups: { id: string; name: string }[];
  can_edit: boolean;
  can_manage_members: boolean;
  /** 항목 정보(리더·목표·범위·기간·상태·종류별 정보)의 판. 고칠 때 연 때의 판을 보낸다(D-104). */
  version: number;
}

const DETAIL_TEXT_KEYS = [
  'field',
  'question',
  'hypothesis',
  'model',
  'method',
  'sample',
  'level',
  'target',
  'activity_type',
] as const;

function parsePerson(value: Json): PersonRefView {
  const o = rec(value, 'person');
  return { user_id: str(o, 'user_id'), display_name: str(o, 'display_name') };
}

export function parseItemProfile(value: Json): ItemProfileView {
  const o = rec(value, 'item profile');
  const d = rec(o.details ?? {}, 'item details');
  const details: ItemDetailsView = {};
  for (const key of DETAIL_TEXT_KEYS) {
    const v = optStr(d, key);
    if (v !== null) details[key] = v;
  }
  if (d.keywords !== undefined) details.keywords = strings(d.keywords);
  return {
    leader: o.leader === null || o.leader === undefined ? null : parsePerson(o.leader),
    goal: str(o, 'goal'),
    scope: str(o, 'scope'),
    starts_on: optStr(o, 'starts_on'),
    ends_on: optStr(o, 'ends_on'),
    status: itemStatus(str(o, 'status')),
    details,
    members: parseList(o.members, parsePerson),
    groups: parseList(o.groups, (g) => {
      const r = rec(g, 'group');
      return { id: str(r, 'id'), name: str(r, 'name') };
    }),
    can_edit: bool(o, 'can_edit'),
    can_manage_members: bool(o, 'can_manage_members'),
    version: int(o, 'version'),
  };
}

/**
 * 편집·초안을 시작한 사용자 id를 서버에 알리는 머리. 서버는 세션의 사용자와 비교만 하고
 * 권한은 세션으로만 정한다.
 */
/** 판 충돌 409가 함께 보낸 서버 최신본(`current`). 판 충돌이 아닌 409(보관 등)와 다른 응답은 undefined다. */
function versionConflict(status: number, json: Json): unknown {
  if (status !== 409 || !isRecord(json) || json.current === undefined) return undefined;
  return json.current;
}

export const modulesApi = {
  itemProfile: (itemId: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(itemId)}/profile`, parseItemProfile),
  /** 항목 정보 고치기. 연 때의 판(`baseVersion`)이 지났으면 아무것도 바꾸지 않고 서버 최신본을 돌려준다(D-104). */
  setItemProfile: async (
    itemId: string,
    baseVersion: number,
    profile: {
      leader_id: string | null;
      goal: string;
      scope: string;
      starts_on: string | null;
      ends_on: string | null;
      status: ItemStatus;
      details: ItemDetailsView;
    },
  ): Promise<{ kind: 'saved' } | { kind: 'conflict'; current: ItemProfileView }> => {
    const { status, json } = await request(
      'PUT',
      `/api/items/${encodeURIComponent(itemId)}/profile`,
      { base_version: baseVersion, ...profile, details: { ...profile.details } },
    );
    const current = versionConflict(status, json);
    if (current !== undefined) return { kind: 'conflict', current: parseItemProfile(current) };
    if (status < 200 || status >= 300) fail(status, json);
    return { kind: 'saved' };
  },
  setItemMembers: (itemId: string, userIds: string[], groupIds: string[]) =>
    expectOk('PUT', `/api/items/${encodeURIComponent(itemId)}/members`, none, {
      user_ids: userIds,
      group_ids: groupIds,
    }),
  /** 항목 복제. reset이면 업무 상태·날짜·체크와 항목 상태·기간을 비운다(템플릿 사용). */
  copyItem: (itemId: string, requestId: string, moduleId: string, title: string, reset: boolean) =>
    expectOk(
      'POST',
      `/api/items/${encodeURIComponent(itemId)}/copy`,
      (v) => {
        const o = rec(v, 'copied item');
        return { item_id: str(o, 'item_id'), skipped_documents: int(o, 'skipped_documents') };
      },
      { request_id: requestId, module_id: moduleId, title, reset },
    ),
  setItemTemplate: (itemId: string, template: boolean) =>
    expectOk('PUT', `/api/items/${encodeURIComponent(itemId)}/template`, none, { template }),
  setModuleIcon: (moduleId: string, icon: string) =>
    expectOk('PUT', `/api/modules/${encodeURIComponent(moduleId)}/icon`, none, { icon }),
  /** 모듈 설명(관리자, D-78). 빈 문자열이면 지운다. */
  setModuleDescription: (moduleId: string, description: string) =>
    expectOk('PUT', `/api/modules/${encodeURIComponent(moduleId)}/description`, parseModule, {
      description,
    }),
  modules: (ws: string) =>
    expectOk('GET', `/api/workspaces/${encodeURIComponent(ws)}/modules`, (v) =>
      parseList(v, parseModule),
    ),
  module: (id: string) => expectOk('GET', `/api/modules/${encodeURIComponent(id)}`, parseModule),
  createModule: (
    ws: string,
    input: { requestId: string; kind: ModuleKind; name: string; key: string },
  ) =>
    expectOk('POST', `/api/workspaces/${encodeURIComponent(ws)}/modules`, parseModule, {
      request_id: input.requestId,
      kind: input.kind,
      name: input.name,
      key: input.key,
    }),
  updateModule: (id: string, name: string, key: string) =>
    expectOk('PUT', `/api/modules/${encodeURIComponent(id)}`, parseModule, { name, key }),
  archiveModule: (id: string, archived: boolean) =>
    expectOk('PUT', `/api/modules/${encodeURIComponent(id)}/archived`, parseModule, { archived }),
  items: (moduleId: string) =>
    expectOk('GET', `/api/modules/${encodeURIComponent(moduleId)}/items`, (v) =>
      parseList(v, parseItem),
    ),
  createItem: (moduleId: string, requestId: string, title: string) =>
    expectOk('POST', `/api/modules/${encodeURIComponent(moduleId)}/items`, parseItem, {
      request_id: requestId,
      title,
    }),
  item: (id: string) => expectOk('GET', `/api/items/${encodeURIComponent(id)}`, parseItem),
  /** 항목 정보(제목·설명·공개 범위) 고치기. 읽은 판(`baseVersion`)이 지났으면 아무것도 바꾸지 않고 서버 최신본을 돌려준다(D-104). */
  updateItem: async (
    id: string,
    baseVersion: number,
    /** 공개 범위는 바꿀 때만 보낸다(없으면 서버의 지금 값 그대로). */
    input: { title: string; description: string; visibility?: 'workspace' | 'private' },
  ): Promise<{ kind: 'updated'; item: ItemView } | { kind: 'conflict'; current: ItemView }> => {
    const { status, json } = await request('PUT', `/api/items/${encodeURIComponent(id)}`, {
      base_version: baseVersion,
      ...input,
    });
    const current = versionConflict(status, json);
    if (current !== undefined) return { kind: 'conflict', current: parseItem(current) };
    if (status !== 200) fail(status, json);
    return { kind: 'updated', item: parseItem(json) };
  },
  archiveItem: (id: string, archived: boolean) =>
    expectOk('PUT', `/api/items/${encodeURIComponent(id)}/archived`, parseItem, { archived }),
};
