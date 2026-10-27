// 개발 seed와 같은 합성 데이터(deploy/postgres/dev-ui-data.json)를 화면 타입으로 옮긴다. 내용(제목·본문·트리·
// 스레드·상태)은 `cargo xtask dev`의 "UI 상태 예제" 워크스페이스와 같다. id·시각은 DB가 정하므로 여기서는 고정값을 쓴다
// (시각은 기준 시각에서 몇 분씩 뒤, 표시 시간대는 preview.ts가 Asia/Seoul로 고정).
import type { MembersView, Module, ModuleKind, Role, Workspace } from '@fvoci/api-client';

import type {
  CalendarView,
  CommentView,
  HomeTaskView,
  HomeView,
  ItemStatus,
  ItemView,
  NotificationView,
  SearchResultView,
  SummaryView,
  TaskPriority,
  TaskStatus,
  TaskView,
} from '$lib/api/client';
import { ITEM_STATUSES, MODULE_KINDS, TASK_PRIORITIES, TASK_STATUSES } from '$lib/api/client';

import data from '../../../../deploy/postgres/dev-ui-data.json';

export const SAMPLES = data.samples;

/** 합성 데이터가 처음 만들어진 것으로 보는 시각. */
const BASE = Date.parse('2026-10-20T01:30:00Z');

function at(minutes: number): string {
  return new Date(BASE + minutes * 60_000).toISOString();
}

/** 사람 열쇠(owner·writer)의 표시 이름. 개발 Keycloak 테스트 사용자의 표시 이름과 같다. */
export function personName(key: string): string {
  return data.people.find((p) => p.key === key)?.name ?? key;
}

/** 문서 트리: 개발 seed가 같은 순서·부모로 만드는 위키 문서. */
export const DOCS: SummaryView[] = data.documents.map((d, i) => ({
  id: `doc-${d.key}`,
  reference: `WIKI-${String(i + 1)}`,
  title: d.title,
  revision: 1,
  updated_at: at(i),
  parent_id: d.parent === null ? null : `doc-${d.parent}`,
}));

/** 댓글 스레드를 owner(Test User, 워크스페이스 소유자)가 보는 화면의 목록으로 편다. 자기 댓글만 고치고, 지우기는 모두 된다(실제 앱과 같음). */
export function commentThreads(): CommentView[] {
  const list: CommentView[] = [];
  let minute = 0;
  data.comments.threads.forEach((thread, t) => {
    const rootId = `c-${String(t + 1)}`;
    const view = (id: string, parent: string | null, author: string, body: string): CommentView => {
      minute += 7;
      return {
        id,
        parent_id: parent,
        body,
        author_id: `u-${author}`,
        author_name: personName(author),
        created_at: at(minute),
        edited: false,
        deleted: false,
        resolved: parent === null && thread.resolved,
        can_edit: author === 'owner',
        can_delete: true,
        can_resolve: parent === null,
      };
    };
    list.push(view(rootId, null, thread.author, thread.body));
    thread.replies.forEach((reply, r) => {
      list.push(view(`${rootId}-${String(r + 1)}`, rootId, reply.author, reply.body));
    });
  });
  return list;
}

/** 업무 합성 데이터(충돌 확인의 서버 값 등에 쓴다). */
export const TASKS = data.tasks;

// ── 화면 단위 story(Screens/)의 워크스페이스·모듈·항목·홈. 개발 seed의 "UI 상태 예제" 워크스페이스와 같은 내용이다. ──

/** 고정 id. 경로 해석(router.svelte.ts)이 UUID 모양만 받으므로 UUID 형태로 만든다. */
function fixedId(group: number, n: number): string {
  return `5e1d0000-0000-4000-8000-${String(group).padStart(4, '0')}${String(n).padStart(8, '0')}`;
}

/** 홈의 "오늘"(서버가 사용자 시간대로 정해 보내는 값). 업무·항목 날짜는 개발 seed처럼 이 날 기준의 앞뒤 일수다. */
export const TODAY = '2026-10-20';

function day(offset: number | null): string | null {
  if (offset === null) return null;
  const d = new Date(`${TODAY}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + offset);
  return d.toISOString().slice(0, 10);
}

export const WORKSPACE: Workspace = {
  id: fixedId(1, 1),
  name: data.workspace,
  organization_id: fixedId(1, 2),
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};

/** 같은 워크스페이스를 다른 역할로 본다(읽기 전용 등). */
export function workspaceAs(role: Role): Workspace {
  return { ...WORKSPACE, role, is_owner: false };
}

interface ModuleData {
  kind: string;
  name: string;
  key: string;
  description: string;
  items: {
    title: string;
    description: string;
    status: string;
    start: number | null;
    end: number | null;
    members: string[];
  }[];
}

/** v1의 프로젝트 모듈(업무가 있는 항목 하나와 v2가 더한 항목들)과 v2의 모듈들. seed가 만드는 순서와 같다. */
const MODULE_DATA: ModuleData[] = [
  {
    kind: 'project',
    name: data.module.name,
    key: data.module.key,
    description: data.module.description,
    items: [],
  },
  ...data.modules,
];

function asKindOf(value: string): ModuleKind {
  const kind = MODULE_KINDS.find((k) => k === value);
  if (kind === undefined) throw new TypeError(`unknown module kind ${value}`);
  return kind;
}

function asItemStatus(value: string): ItemStatus {
  const status = ITEM_STATUSES.find((s) => s === value);
  if (status === undefined) throw new TypeError(`unknown item status ${value}`);
  return status;
}

function asTaskStatus(value: string): TaskStatus {
  const status = TASK_STATUSES.find((s) => s === value);
  if (status === undefined) throw new TypeError(`unknown task status ${value}`);
  return status;
}

function asTaskPriority(value: string): TaskPriority {
  const priority = TASK_PRIORITIES.find((p) => p === value);
  if (priority === undefined) throw new TypeError(`unknown task priority ${value}`);
  return priority;
}

export const MODULES: Module[] = MODULE_DATA.map((mod, i) => ({
  id: fixedId(2, i + 1),
  workspace_id: WORKSPACE.id,
  kind: asKindOf(mod.kind),
  name: mod.name,
  key: mod.key,
  previous_keys: [],
  archived: false,
  icon: '',
  description: mod.description,
}));

function moduleAt(index: number): Module {
  const mod = MODULES[index];
  if (mod === undefined) throw new RangeError(`no module ${String(index)}`);
  return mod;
}

/** 프로젝트 모듈(UI 예제, 업무가 있는 항목이 든 모듈). */
export const PROJECT_MODULE = moduleAt(0);

/** 모듈 종류마다 처음 나오는 모듈(프로젝트·연구·스터디·강의·취미). */
export const ONE_PER_KIND: Module[] = MODULE_KINDS.flatMap((kind) => {
  const found = MODULES.find((mod) => mod.kind === kind);
  return found === undefined ? [] : [found];
});

/** 항목이 없는 모듈(이모지 이름). */
export const EMPTY_MODULE = moduleAt(MODULES.length - 1);

function item(
  mod: Module,
  n: number,
  number: number,
  fields: Pick<ItemView, 'title' | 'description' | 'status' | 'starts_on' | 'ends_on'>,
): ItemView {
  return {
    id: fixedId(3, mod === PROJECT_MODULE ? n : MODULES.indexOf(mod) * 100 + n),
    module_id: mod.id,
    workspace_id: WORKSPACE.id,
    reference: `${mod.key}-${String(number)}`,
    visibility: 'private',
    access: 'owner',
    archived: false,
    updated_at: at(n),
    is_template: false,
    version: 1,
    ...fields,
  };
}

/** 업무가 든 항목(개발 seed v1). 번호는 모듈 안에서 항목·업무가 함께 센다: 항목 1, 업무 2~8. */
export const TASK_ITEM = item(PROJECT_MODULE, 1, 1, {
  title: data.module.item,
  description: data.module.item_description,
  status: 'active',
  starts_on: null,
  ends_on: null,
});

/** 모듈마다의 항목 목록(서버 순서: 만든 순서). */
export function itemsOf(mod: Module): ItemView[] {
  const index = MODULES.indexOf(mod);
  const list = index === 0 ? data.project_items : (MODULE_DATA[index]?.items ?? []);
  // 프로젝트 모듈은 업무 다음 번호부터(9~).
  const first = index === 0 ? data.tasks.length + 2 : 1;
  const made = list.map((it, i) =>
    item(mod, i + 2, first + i, {
      title: it.title,
      description: it.description,
      status: asItemStatus(it.status),
      starts_on: day(it.start),
      ends_on: day(it.end),
    }),
  );
  return index === 0 ? [TASK_ITEM, ...made] : made;
}

/** 업무(프로젝트 모듈 첫 항목). 홈·달력의 업무 줄에 쓴다. */
const TASK_ROWS = data.tasks.map((t, i) => ({
  id: fixedId(4, i + 1),
  item_id: TASK_ITEM.id,
  workspace_id: WORKSPACE.id,
  reference: `${PROJECT_MODULE.key}-${String(i + 2)}`,
  title: t.title,
  status: asTaskStatus(t.status),
  milestone: t.milestone,
  start_date: day(t.start),
  due_date: day(t.due),
  // 서버(features/home.rs의 UP_NEXT_MINE)처럼 내 담당이거나, 담당자가 없고 내가 만든 업무(seed는 모두 Test User가 만든다).
  mine: t.assignees.includes('owner') || t.assignees.length === 0,
}));

function bucketOf(due: string | null): HomeTaskView['bucket'] {
  if (due === null) return null;
  if (due < TODAY) return 'overdue';
  return due === TODAY ? 'today' : 'upcoming';
}

const homeTask = (t: (typeof TASK_ROWS)[number]): HomeTaskView => ({
  id: t.id,
  item_id: t.item_id,
  workspace_id: t.workspace_id,
  reference: t.reference,
  title: t.title,
  status: t.status,
  due_date: t.due_date,
  bucket: bucketOf(t.due_date),
});

/** 홈: 내 업무(Test User 담당, 끝나지 않음)의 다음 할 일, 고정·즐겨찾기, 진행 중인 항목, 최근 문서·업무. */
export function homeView(): HomeView {
  // 서버처럼 오늘부터 7일 뒤까지 마감인 것만, 마감이 이른 것부터(같으면 번호 순).
  const until = day(7) ?? TODAY;
  const upNext = TASK_ROWS.filter(
    (t) => t.mine && t.status !== 'done' && t.due_date !== null && t.due_date <= until,
  )
    .map(homeTask)
    .sort((a, b) => (a.due_date ?? '').localeCompare(b.due_date ?? ''));
  const count = (b: 'overdue' | 'today' | 'upcoming'): number =>
    upNext.filter((t) => t.bucket === b).length;
  const doc = (key: string): SummaryView => {
    const found = DOCS.find((d) => d.id === `doc-${key}`);
    if (found === undefined) throw new RangeError(key);
    return found;
  };
  const active = MODULES.flatMap((mod) =>
    itemsOf(mod)
      .filter((it) => it.status === 'active')
      .map((it) => ({
        id: it.id,
        workspace_id: it.workspace_id,
        module_kind: mod.kind,
        reference: it.reference ?? null,
        title: it.title,
        tasks_done: it === TASK_ITEM ? TASK_ROWS.filter((t) => t.status === 'done').length : 0,
        tasks_total: it === TASK_ITEM ? TASK_ROWS.length : 0,
        can_write: true,
      })),
  );
  return {
    today: TODAY,
    up_next: upNext,
    up_next_counts: {
      overdue: count('overdue'),
      today: count('today'),
      upcoming: count('upcoming'),
    },
    recent_tasks: TASK_ROWS.slice(0, 3).map(homeTask),
    recent_documents: ['seminar', 'long-title', 'no-space', 'emoji'].map((key) => {
      const d = doc(key);
      return {
        id: d.id,
        title: d.title,
        reference: d.reference,
        workspace_id: WORKSPACE.id,
        item_id: null,
        updated_at: d.updated_at,
      };
    }),
    active_items: active,
    pins: [
      {
        kind: 'document',
        id: doc('guide').id,
        label: doc('guide').title,
        workspace_id: WORKSPACE.id,
        item_id: null,
      },
      {
        kind: 'item',
        id: TASK_ITEM.id,
        label: TASK_ITEM.title,
        workspace_id: WORKSPACE.id,
        item_id: null,
      },
    ],
    favorites: [
      {
        kind: 'document',
        id: doc('long-title').id,
        label: doc('long-title').title,
        workspace_id: WORKSPACE.id,
        item_id: null,
      },
    ],
  };
}

/** 처음 쓰는 사람의 홈: 오늘 날짜만 있다. */
export function emptyHomeView(): HomeView {
  return {
    today: TODAY,
    up_next: [],
    up_next_counts: { overdue: 0, today: 0, upcoming: 0 },
    recent_tasks: [],
    recent_documents: [],
    active_items: [],
    pins: [],
    favorites: [],
  };
}

/** 워크스페이스 달력(홈의 "이번 주"): 날짜가 있는 업무만. */
export function calendarView(): CalendarView {
  return {
    tasks: TASK_ROWS.filter((t) => t.due_date !== null || t.start_date !== null).map((t) => ({
      id: t.id,
      item_id: t.item_id,
      reference: t.reference,
      title: t.title,
      status: t.status,
      milestone: t.milestone,
      start_date: t.start_date,
      due_date: t.due_date,
    })),
    projected_tasks: [],
    events: [],
    occurrences: [],
    dates: [],
    items: [{ id: TASK_ITEM.id, reference: TASK_ITEM.reference ?? null, title: TASK_ITEM.title }],
  };
}

export function emptyCalendarView(): CalendarView {
  return { tasks: [], projected_tasks: [], events: [], occurrences: [], dates: [], items: [] };
}

/** 사람 열쇠를 화면의 사람으로. id는 댓글 작성자(commentThreads)와 같다. */
function person(key: string): { user_id: string; display_name: string } {
  return { user_id: `u-${key}`, display_name: personName(key) };
}

/**
 * 업무 항목의 업무 전체(칸반·간트). 개발 seed가 같은 순서로 만드는 업무이고 id·번호는 홈·달력의 업무 줄과 같다.
 * seed에는 선행 관계가 없으므로 연결선이 필요한 story가 `predecessors`를 더한다.
 */
export function taskViews(): TaskView[] {
  return data.tasks.map((t, i) => {
    const row = TASK_ROWS[i];
    if (row === undefined) throw new RangeError(`no task row ${String(i)}`);
    return {
      id: row.id,
      item_id: TASK_ITEM.id,
      module_id: PROJECT_MODULE.id,
      workspace_id: WORKSPACE.id,
      parent_id: null,
      reference: row.reference,
      title: t.title,
      description: t.description,
      status: row.status,
      priority: asTaskPriority(t.priority),
      start_date: row.start_date,
      due_date: row.due_date,
      estimate_minutes: t.estimate,
      tags: t.tags,
      milestone: t.milestone,
      predecessors: [],
      series: null,
      occurrence_date: null,
      assignees: t.assignees.map(person),
      checklist_done: 0,
      checklist_total: 0,
      version: 1,
      move_version: 1,
      completed_at: row.status === 'done' ? at(10 + i) : null,
      access: 'write',
      updated_at: at(10 + i),
    };
  });
}

/**
 * 검색 결과 줄(서버가 고른 결과를 그대로 보이는 화면용). 문서는 본문 앞부분을 요약으로, 업무는 설명을 요약으로 둔다.
 * 검색어로 거르거나 순서를 정하지 않는다(서버의 일이다). story가 필요한 만큼 잘라 쓴다.
 */
export function searchResults(): SearchResultView[] {
  const docs = data.documents.map((d, i): SearchResultView => {
    const doc = DOCS[i];
    if (doc === undefined) throw new RangeError(`no document ${d.key}`);
    return {
      kind: 'document',
      id: doc.id,
      title: d.title,
      reference: doc.reference,
      workspace_id: WORKSPACE.id,
      item_id: null,
      document_id: doc.id,
      snippet: d.body === '' ? null : d.body.slice(0, 200),
    };
  });
  const tasks = taskViews().map((t): SearchResultView => ({
    kind: 'task',
    id: t.id,
    title: t.title,
    reference: t.reference,
    workspace_id: WORKSPACE.id,
    item_id: TASK_ITEM.id,
    document_id: null,
    snippet: t.description,
  }));
  return [...docs, ...tasks];
}

/** 워크스페이스 멤버(owner는 소유자·관리자, writer는 작성자). 개발 seed의 두 테스트 사용자와 같다. */
export function membersView(): MembersView {
  return {
    members: [
      { ...person('owner'), email: null, role: 'admin', is_owner: true },
      { ...person('writer'), email: null, role: 'member', is_owner: false },
    ],
    my_role: 'admin',
    i_am_owner: true,
    pending_owner: null,
  };
}

/**
 * 읽지 않은 알림: 댓글 스레드에서 Second User가 owner에게 남긴 답글·댓글과, 지난 마감 업무. 개발 seed에서는 같은 댓글이
 * 실제 알림을 만든다(마감 알림은 작업 프로세스가 정한 때 만든다).
 */
export function notificationsView(): NotificationView[] {
  const seminar = DOCS.find((d) => d.id === `doc-${data.comments.document}`);
  const list: NotificationView[] = [];
  data.comments.threads.forEach((thread, t) => {
    const rows = thread.author === 'writer' ? [{ kind: 'comment', body: thread.body }] : [];
    thread.replies.forEach((reply) => {
      if (reply.author === 'writer' && thread.author === 'owner')
        rows.push({ kind: 'reply', body: reply.body });
    });
    rows.forEach((row, r) => {
      list.push({
        id: `n-${String(t + 1)}-${String(r + 1)}`,
        kind: row.kind,
        document_id: seminar?.id ?? null,
        task_id: null,
        document_title: seminar?.title ?? '',
        document_reference: seminar?.reference ?? null,
        workspace_id: WORKSPACE.id,
        item_id: null,
        comment_id: null,
        excerpt: row.body,
        due_date: null,
        subject: null,
        actor_name: personName('writer'),
        created_at: at(30 + list.length),
        read: false,
      });
    });
  });
  const overdue = TASK_ROWS.find(
    (t) => t.mine && t.status !== 'done' && bucketOf(t.due_date) === 'overdue',
  );
  if (overdue !== undefined)
    list.push({
      id: 'n-due',
      kind: 'due',
      document_id: null,
      task_id: overdue.id,
      document_title: overdue.title,
      document_reference: overdue.reference,
      workspace_id: WORKSPACE.id,
      item_id: overdue.item_id,
      comment_id: null,
      excerpt: null,
      due_date: overdue.due_date,
      subject: null,
      actor_name: null,
      created_at: at(60),
      read: false,
    });
  return list;
}
