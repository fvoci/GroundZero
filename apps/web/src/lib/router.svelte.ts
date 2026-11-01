// 최소 경로 라우터. SvelteKit을 쓰지 않으므로(D-66) history API로 경로만 관리한다.
// 서버는 알 수 없는 경로에 index.html을 돌려 새로고침을 지원한다.
import type { ModuleKind } from '@fvoci/api-client';

/** 항목의 업무 보기(D-121). 하위 경로 `/tasks/<view>`로 고르고, 없으면 저장한 보기 › 세션 순으로 정한다. */
export type TaskViewName = 'board' | 'list' | 'calendar' | 'gantt';
export const TASK_VIEWS: readonly TaskViewName[] = ['board', 'list', 'calendar', 'gantt'];

/**
 * 항목 화면의 묶음(D-121, 사용자 R7 A). 종류별 첫 묶음(연구 문헌, 스터디 계획, 강의 회차, 취미 활동) 하나 뒤에 공통
 * 업무·문서·일정·기록이 온다. 프로젝트는 업무가 첫 묶음이다. 업무 묶음의 경로는 `/tasks/<view>`이고 나머지는
 * `/<묶음>`이다.
 */
export type ItemGroupName =
  'tasks' | 'docs' | 'events' | 'records' | 'references' | 'plan' | 'episodes' | 'activities';
const COMMON_GROUPS: readonly ItemGroupName[] = ['tasks', 'docs', 'events', 'records'];
const KIND_GROUP: Record<ModuleKind, ItemGroupName | undefined> = {
  project: undefined,
  research: 'references',
  study: 'plan',
  course: 'episodes',
  hobby: 'activities',
};
/** 경로 없이 항목을 열고 기억한 묶음도 없을 때의 묶음(첫 묶음). */
export const ITEM_GROUP_DEFAULT: Record<ModuleKind, ItemGroupName> = {
  project: 'tasks',
  research: 'references',
  study: 'plan',
  course: 'episodes',
  hobby: 'activities',
};

/** 모듈 종류에 있는 묶음을 단추 차례대로. */
export function itemGroupsFor(kind: ModuleKind): ItemGroupName[] {
  const first = KIND_GROUP[kind];
  return first === undefined ? [...COMMON_GROUPS] : [first, ...COMMON_GROUPS];
}

/** 모듈 화면의 보기(D-121, 사용자 R7 A): 항목(`/m/:mod`)과 달력(`/m/:mod/calendar`). */
export type ModuleViewName = 'items' | 'calendar';
export type OrganizationTab = 'members' | 'invitations' | 'workspaces' | 'settings';
export type WorkspaceManagementTab = 'members' | 'invitations' | 'groups' | 'settings';
export type ServerAdminTab = 'organizations' | 'admins';

export type Route =
  | { name: 'home' }
  | { name: 'organization'; organizationId: string; tab: OrganizationTab }
  | { name: 'server-admin'; tab: ServerAdminTab }
  | { name: 'my-tasks' }
  | { name: 'workspace'; workspaceId: string }
  | { name: 'document'; workspaceId: string; documentId: string }
  | { name: 'members'; workspaceId: string; tab: WorkspaceManagementTab }
  | { name: 'calendar'; workspaceId: string }
  | { name: 'trash'; workspaceId: string }
  | { name: 'my-trash' }
  | { name: 'audit'; workspaceId: string }
  | { name: 'my-audit' }
  /** 모듈 화면. `view`가 없으면 항목 보기다. */
  | { name: 'module'; workspaceId: string; moduleId: string; view?: 'calendar' }
  /**
   * 항목 화면. `group`은 주소의 하위 경로가 가리키는 묶음이고 없으면(뿌리 경로) 화면이 세션 기억·종류별 기본 묶음으로
   * 주소를 바꾼다. `view`는 업무 묶음의 업무 보기(`/tasks/board` 등)다. 업무 상세는 보드 보기 아래 `/tasks/board/t/:taskId`다.
   */
  | {
      name: 'item';
      workspaceId: string;
      itemId: string;
      group?: ItemGroupName;
      view?: TaskViewName;
      taskId?: string;
    }
  | { name: 'item-document'; workspaceId: string; itemId: string; documentId: string }
  | { name: 'notes' }
  | { name: 'shared' }
  | { name: 'notifications'; notificationId?: string }
  | { name: 'time' }
  | { name: 'standalone-document'; documentId: string }
  | { name: 'public'; token: string }
  | { name: 'not-found' };

const PUBLIC = /^\/p\/([A-Za-z0-9_-]{16,128})\/?$/;

const UUID = '[0-9a-fA-F-]{36}';
const ORGANIZATION = new RegExp(`^/o/(${UUID})(?:/(members|invitations|workspaces|settings))?/?$`);
const WORKSPACE = new RegExp(`^/w/(${UUID})/?$`);
const DOCUMENT = new RegExp(`^/w/(${UUID})/wiki/(${UUID})/?$`);
const MEMBERS = new RegExp(`^/w/(${UUID})/(members|invitations|groups|settings)/?$`);
const CALENDAR = new RegExp(`^/w/(${UUID})/calendar/?$`);
const TRASH = new RegExp(`^/w/(${UUID})/trash/?$`);
const AUDIT = new RegExp(`^/w/(${UUID})/audit/?$`);
const STANDALONE = new RegExp(`^/d/(${UUID})/?$`);
const MODULE = new RegExp(`^/w/(${UUID})/m/(${UUID})/?$`);
const MODULE_CALENDAR = new RegExp(`^/w/(${UUID})/m/(${UUID})/calendar/?$`);
const ITEM = new RegExp(`^/w/(${UUID})/i/(${UUID})/?$`);
const ITEM_GROUP = new RegExp(
  `^/w/(${UUID})/i/(${UUID})/(docs|events|records|references|plan|episodes|activities)/?$`,
);
const ITEM_VIEW = new RegExp(`^/w/(${UUID})/i/(${UUID})/tasks/(board|list|calendar|gantt)/?$`);
const ITEM_TASK = new RegExp(`^/w/(${UUID})/i/(${UUID})/tasks/board/t/(${UUID})/?$`);
const ITEM_DOCUMENT = new RegExp(`^/w/(${UUID})/i/(${UUID})/d/(${UUID})/?$`);
const NOTIFICATION = new RegExp(`^/notifications/(${UUID})/?$`);

function taskView(raw: string): TaskViewName {
  if (raw === 'board' || raw === 'list' || raw === 'calendar' || raw === 'gantt') return raw;
  throw new TypeError(`unknown task view ${raw}`);
}

function itemGroup(raw: string): Exclude<ItemGroupName, 'tasks'> {
  if (
    raw === 'docs' ||
    raw === 'events' ||
    raw === 'records' ||
    raw === 'references' ||
    raw === 'plan' ||
    raw === 'episodes' ||
    raw === 'activities'
  )
    return raw;
  throw new TypeError(`unknown item group ${raw}`);
}

export function parseRoute(pathname: string): Route {
  if (pathname === '/' || pathname === '') return { name: 'home' };
  const admin = /^\/admin(?:\/(organizations|admins))?\/?$/.exec(pathname);
  if (admin !== null)
    return { name: 'server-admin', tab: admin[1] === 'admins' ? 'admins' : 'organizations' };
  const organization = ORGANIZATION.exec(pathname);
  if (organization?.[1] !== undefined) {
    const tab = organization[2];
    if (
      tab === undefined ||
      tab === 'members' ||
      tab === 'invitations' ||
      tab === 'workspaces' ||
      tab === 'settings'
    )
      return { name: 'organization', organizationId: organization[1], tab: tab ?? 'members' };
  }
  const pub = PUBLIC.exec(pathname);
  if (pub?.[1] !== undefined) return { name: 'public', token: pub[1] };
  if (pathname === '/tasks' || pathname === '/tasks/') return { name: 'my-tasks' };
  if (pathname === '/notes' || pathname === '/notes/') return { name: 'notes' };
  if (pathname === '/shared' || pathname === '/shared/') return { name: 'shared' };
  if (pathname === '/time' || pathname === '/time/') return { name: 'time' };
  if (pathname === '/trash' || pathname === '/trash/') return { name: 'my-trash' };
  if (pathname === '/audit' || pathname === '/audit/') return { name: 'my-audit' };
  if (pathname === '/notifications' || pathname === '/notifications/')
    return { name: 'notifications' };
  const notification = NOTIFICATION.exec(pathname);
  if (notification?.[1] !== undefined) {
    return { name: 'notifications', notificationId: notification[1] };
  }
  const standalone = STANDALONE.exec(pathname);
  if (standalone?.[1] !== undefined) {
    return { name: 'standalone-document', documentId: standalone[1] };
  }
  const mod = MODULE.exec(pathname);
  if (mod?.[1] !== undefined && mod[2] !== undefined) {
    return { name: 'module', workspaceId: mod[1], moduleId: mod[2] };
  }
  const modCalendar = MODULE_CALENDAR.exec(pathname);
  if (modCalendar?.[1] !== undefined && modCalendar[2] !== undefined) {
    return {
      name: 'module',
      workspaceId: modCalendar[1],
      moduleId: modCalendar[2],
      view: 'calendar',
    };
  }
  const item = ITEM.exec(pathname);
  if (item?.[1] !== undefined && item[2] !== undefined) {
    return { name: 'item', workspaceId: item[1], itemId: item[2] };
  }
  const group = ITEM_GROUP.exec(pathname);
  if (group?.[1] !== undefined && group[2] !== undefined && group[3] !== undefined) {
    return { name: 'item', workspaceId: group[1], itemId: group[2], group: itemGroup(group[3]) };
  }
  const itemView = ITEM_VIEW.exec(pathname);
  if (itemView?.[1] !== undefined && itemView[2] !== undefined && itemView[3] !== undefined) {
    return {
      name: 'item',
      workspaceId: itemView[1],
      itemId: itemView[2],
      group: 'tasks',
      view: taskView(itemView[3]),
    };
  }
  const itemTask = ITEM_TASK.exec(pathname);
  if (itemTask?.[1] !== undefined && itemTask[2] !== undefined && itemTask[3] !== undefined) {
    return {
      name: 'item',
      workspaceId: itemTask[1],
      itemId: itemTask[2],
      group: 'tasks',
      view: 'board',
      taskId: itemTask[3],
    };
  }
  const itemDoc = ITEM_DOCUMENT.exec(pathname);
  if (itemDoc?.[1] !== undefined && itemDoc[2] !== undefined && itemDoc[3] !== undefined) {
    return {
      name: 'item-document',
      workspaceId: itemDoc[1],
      itemId: itemDoc[2],
      documentId: itemDoc[3],
    };
  }
  const tr = TRASH.exec(pathname);
  if (tr?.[1] !== undefined) return { name: 'trash', workspaceId: tr[1] };
  const audit = AUDIT.exec(pathname);
  if (audit?.[1] !== undefined) return { name: 'audit', workspaceId: audit[1] };
  const cal = CALENDAR.exec(pathname);
  if (cal?.[1] !== undefined) return { name: 'calendar', workspaceId: cal[1] };
  const members = MEMBERS.exec(pathname);
  if (members?.[1] !== undefined) {
    const tab = members[2];
    if (tab === 'members' || tab === 'invitations' || tab === 'groups' || tab === 'settings')
      return { name: 'members', workspaceId: members[1], tab };
  }
  const doc = DOCUMENT.exec(pathname);
  if (doc?.[1] !== undefined && doc[2] !== undefined) {
    return { name: 'document', workspaceId: doc[1], documentId: doc[2] };
  }
  const ws = WORKSPACE.exec(pathname);
  if (ws?.[1] !== undefined) return { name: 'workspace', workspaceId: ws[1] };
  return { name: 'not-found' };
}

export function workspacePath(workspaceId: string): string {
  return `/w/${workspaceId}`;
}

export function serverAdminPath(tab: ServerAdminTab = 'organizations'): string {
  return `/admin/${tab}`;
}

export function organizationPath(organizationId: string, tab: OrganizationTab = 'members'): string {
  return `/o/${organizationId}/${tab}`;
}

export function membersPath(workspaceId: string, tab: WorkspaceManagementTab = 'members'): string {
  return `/w/${workspaceId}/${tab}`;
}

export function calendarPath(workspaceId: string): string {
  return `/w/${workspaceId}/calendar`;
}

export function trashPath(workspaceId: string): string {
  return `/w/${workspaceId}/trash`;
}

export function auditPath(workspaceId: string): string {
  return `/w/${workspaceId}/audit`;
}

export function documentPath(workspaceId: string, documentId: string): string {
  return `/w/${workspaceId}/wiki/${documentId}`;
}

/** 워크스페이스 밖에서 여는 문서(개인 노트, 공유받은 문서)의 경로. */
export function standalonePath(documentId: string): string {
  return `/d/${documentId}`;
}

export function modulePath(workspaceId: string, moduleId: string): string {
  return `/w/${workspaceId}/m/${moduleId}`;
}

/** 모듈의 달력 보기(D-121): `/w/:ws/m/:mod/calendar`. */
export function moduleCalendarPath(workspaceId: string, moduleId: string): string {
  return `/w/${workspaceId}/m/${moduleId}/calendar`;
}

/** 항목 뿌리 경로. 화면이 세션 기억 › 종류별 기본 묶음 경로로 바꾼다(replaceState). */
export function itemPath(workspaceId: string, itemId: string): string {
  return `/w/${workspaceId}/i/${itemId}`;
}

/** 항목 묶음 경로(D-121): 업무 묶음은 `view`(기본 보드) 보기 경로, 나머지는 `/w/:ws/i/:item/<묶음>`. */
export function itemGroupPath(
  workspaceId: string,
  itemId: string,
  group: ItemGroupName,
  view: TaskViewName = 'board',
): string {
  return group === 'tasks'
    ? itemViewPath(workspaceId, itemId, view)
    : `/w/${workspaceId}/i/${itemId}/${group}`;
}

/** `next`가 같은 항목 화면 안의 경로인지(묶음·업무 보기만 바뀌어 항목 화면이 그대로 마운트된 채인지). */
export function staysOnItem(next: string, itemId: string): boolean {
  const route = parseRoute(next);
  return route.name === 'item' && route.itemId === itemId;
}

/** `next`가 같은 모듈 화면 안의 경로인지(항목·달력 보기만 바뀌는지). */
export function staysOnModule(next: string, moduleId: string): boolean {
  const route = parseRoute(next);
  return route.name === 'module' && route.moduleId === moduleId;
}

/** 항목의 업무 보기 경로(D-121): `/w/:ws/i/:item/tasks/board|list|calendar|gantt`. */
export function itemViewPath(workspaceId: string, itemId: string, view: TaskViewName): string {
  return `/w/${workspaceId}/i/${itemId}/tasks/${view}`;
}

/** 업무 상세는 보드 보기 아래 `/tasks/board/t/:taskId`다(D-121). */
export function itemTaskPath(workspaceId: string, itemId: string, taskId: string): string {
  return `/w/${workspaceId}/i/${itemId}/tasks/board/t/${taskId}`;
}

export function myTasksPath(): string {
  return '/tasks';
}

export function notificationPath(notificationId: string): string {
  return `/notifications/${notificationId}`;
}

export function itemDocumentPath(workspaceId: string, itemId: string, documentId: string): string {
  return `/w/${workspaceId}/i/${itemId}/d/${documentId}`;
}

/** 문서가 놓인 곳에 맞는 경로. 내가 멤버인 워크스페이스 문서는 워크스페이스 경로로 연다. */
export function pathFor(
  doc: { id: string; workspace_id: string | null; item_id?: string | null },
  memberOf: (ws: string) => boolean,
): string {
  if (doc.workspace_id === null || !memberOf(doc.workspace_id)) return standalonePath(doc.id);
  const itemId = doc.item_id ?? null;
  return itemId === null
    ? documentPath(doc.workspace_id, doc.id)
    : itemDocumentPath(doc.workspace_id, itemId, doc.id);
}

class Router {
  private currentPath = $state(window.location.pathname);
  /** Accepted path changes are synchronous, including A → B → A within one render tick. */
  navigationGeneration = 0;
  get path(): string {
    return this.currentPath;
  }
  set path(next: string) {
    if (next === this.currentPath) return;
    this.navigationGeneration += 1;
    this.currentPath = next;
  }
  /** 주소의 `#` 부분(백링크가 원문 문서 안의 참조 자리를 가리킬 때). 경로 해석에는 쓰지 않는다. */
  hash = $state(window.location.hash);
  route = $derived(parseRoute(this.path));
  /**
   * 이동 전에 확인할 함수(미저장 변경 보호). false면 이동하지 않는다. 막은 뒤 확인 대화상자에서 떠나기로 하면 `proceed`로
   * 같은 이동을 다시 한다(그때는 가드가 true를 돌려줘야 한다). `next`는 갈 경로다(앱 안 이동·뒤로 가기). 경로를 모르는
   * 문맥 전환·기록 뒤로 가기에서는 없다. 다음 경로에서도 마운트된 채인 폼은 이 값으로 묻지 않고 통과한다.
   */
  guard: ((proceed: () => void, next?: string) => boolean) | undefined = undefined;
  /**
   * 이미 가드를 거친 기록 이동이 일으킬 popstate 한 번은 다시 묻지 않는다: 가드가 막은 뒤로 가기를 떠나기로 확정했을 때
   * (아래 참고)와 가드가 허락한 `back()`(RA-1: 묻고 버린 입력이나 다음 화면에 남는 입력을 그 popstate가 다시 물었다).
   */
  private backPermit: { from: string; to: string | undefined } | undefined;

  constructor() {
    // 뒤로·앞으로 가기도 미저장 편집 보호를 거친다(D-121). 브라우저는 주소를 먼저 바꾸므로, 가드가 막으면 같은 기록을
    // 다시 쌓아 원래 주소로 되돌리고 화면은 그대로 둔다. 확인 대화상자에서 떠나기로 하면 `proceed`가 다시 뒤로 간다.
    window.addEventListener('popstate', () => {
      const next = window.location.pathname;
      const nextHash = window.location.hash;
      const permit = this.backPermit;
      this.backPermit = undefined;
      const permitted =
        permit?.from === this.path && (permit.to === undefined || permit.to === next);
      if (next !== this.path && this.guard !== undefined && !permitted) {
        const guard = this.guard;
        const from = this.path;
        let resumed = false;
        const allowed = guard(() => {
          if (resumed || this.path !== from) return;
          resumed = true;
          this.backPermit = { from, to: next };
          window.history.back();
        }, next);
        if (!allowed) {
          window.history.pushState(null, '', this.path + this.hash);
          return;
        }
      }
      this.path = next;
      this.hash = nextHash;
    });
  }

  navigate(target: string): void {
    this.backPermit = undefined;
    const at = target.indexOf('#');
    const path = at < 0 ? target : target.slice(0, at);
    const hash = at < 0 ? '' : target.slice(at);
    if (path === this.path && hash === this.hash) return;
    // 같은 화면 안에서 `#`만 바뀌면 화면을 떠나지 않으므로 미저장 확인을 묻지 않는다.
    if (
      path !== this.path &&
      this.guard !== undefined &&
      !this.guard(() => {
        this.navigate(target);
      }, path)
    )
      return;
    window.history.pushState(null, '', target);
    this.path = path;
    this.hash = hash;
  }

  /**
   * 지금 화면의 주소를 바꾼다(replaceState). 항목 뿌리 경로를 기억한 묶음·기본 묶음 경로로 바꾸는 것처럼 화면을 떠나지 않는
   * 정규화라 이탈 확인을 거치지 않고 기록도 쌓지 않는다.
   */
  replace(target: string): void {
    this.backPermit = undefined;
    const at = target.indexOf('#');
    const path = at < 0 ? target : target.slice(0, at);
    const hash = at < 0 ? '' : target.slice(at);
    if (path === this.path && hash === this.hash) return;
    window.history.replaceState(null, '', target);
    this.path = path;
    this.hash = hash;
  }

  /**
   * 이동이 아닌 문맥 전환(조직 바꾸기 등)도 같은 이탈 확인을 거친다(D-121·D-125). 가드가 없거나 허락하면 바로 실행하고,
   * 가드가 막으면 사용자가 떠나기로 했을 때 가드가 `action`을 실행한다. 취소하면 아무것도 바꾸지 않는다. `action` 안의
   * `navigate`는 그 확정 안에서 다시 묻지 않는다(가드가 proceed 동안 통과시킨다).
   */
  confirmLeave(action: () => void): void {
    this.backPermit = undefined;
    if (this.guard !== undefined && !this.guard(action)) return;
    action();
  }

  /**
   * 바로 앞 기록으로 돌아간다(이 화면이 기록을 쌓아 연 대화상자를 닫을 때). 주소는 popstate가 맞춘다. 가드가 막으면 false.
   * 앞 기록이 있을 때만 부른다(없으면 popstate가 오지 않아 다음 뒤로 가기가 가드를 건너뛴다). `next`는 그 앞 기록의
   * 경로다. 넘기면 그 경로에서도 마운트된 채인 폼이 묻지 않고 통과한다(RF-1: 항목 머리 폼).
   */
  back(next?: string): boolean {
    if (this.backPermit?.from === this.path) return true;
    if (
      this.guard !== undefined &&
      !this.guard(() => {
        this.back(next);
      }, next)
    )
      return false;
    this.backPermit = { from: this.path, to: next };
    window.history.back();
    return true;
  }
}

export const router = new Router();
