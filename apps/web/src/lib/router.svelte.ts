// 최소 경로 라우터. SvelteKit을 쓰지 않으므로(D-66) history API로 경로만 관리한다.
// 서버는 알 수 없는 경로에 index.html을 돌려 새로고침을 지원한다.

/** 항목의 업무 보기(D-121). 하위 경로 `/tasks/<view>`로 고르고, 없으면 저장한 보기 › 세션 순으로 정한다. */
export type TaskViewName = 'board' | 'list' | 'calendar' | 'gantt';
export const TASK_VIEWS: readonly TaskViewName[] = ['board', 'list', 'calendar', 'gantt'];

export type Route =
  | { name: 'home' }
  | { name: 'my-tasks' }
  | { name: 'workspace'; workspaceId: string }
  | { name: 'document'; workspaceId: string; documentId: string }
  | { name: 'members'; workspaceId: string }
  | { name: 'calendar'; workspaceId: string }
  | { name: 'trash'; workspaceId: string }
  | { name: 'my-trash' }
  | { name: 'audit'; workspaceId: string }
  | { name: 'my-audit' }
  | { name: 'module'; workspaceId: string; moduleId: string }
  /**
   * 항목 화면. `view`는 주소의 하위 경로(`/tasks/board` 등)가 가리키는 업무 보기이고 없으면 화면이 저장한 보기·세션으로
   * 정한다. 업무 상세는 보드 보기 아래 `/tasks/board/t/:taskId`다.
   */
  | { name: 'item'; workspaceId: string; itemId: string; view?: TaskViewName; taskId?: string }
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
const WORKSPACE = new RegExp(`^/w/(${UUID})/?$`);
const DOCUMENT = new RegExp(`^/w/(${UUID})/wiki/(${UUID})/?$`);
const MEMBERS = new RegExp(`^/w/(${UUID})/members/?$`);
const CALENDAR = new RegExp(`^/w/(${UUID})/calendar/?$`);
const TRASH = new RegExp(`^/w/(${UUID})/trash/?$`);
const AUDIT = new RegExp(`^/w/(${UUID})/audit/?$`);
const STANDALONE = new RegExp(`^/d/(${UUID})/?$`);
const MODULE = new RegExp(`^/w/(${UUID})/m/(${UUID})/?$`);
const ITEM = new RegExp(`^/w/(${UUID})/i/(${UUID})/?$`);
const ITEM_VIEW = new RegExp(`^/w/(${UUID})/i/(${UUID})/tasks/(board|list|calendar|gantt)/?$`);
const ITEM_TASK = new RegExp(`^/w/(${UUID})/i/(${UUID})/tasks/board/t/(${UUID})/?$`);
const ITEM_DOCUMENT = new RegExp(`^/w/(${UUID})/i/(${UUID})/d/(${UUID})/?$`);
const NOTIFICATION = new RegExp(`^/notifications/(${UUID})/?$`);

function taskView(raw: string): TaskViewName {
  if (raw === 'board' || raw === 'list' || raw === 'calendar' || raw === 'gantt') return raw;
  throw new TypeError(`unknown task view ${raw}`);
}

export function parseRoute(pathname: string): Route {
  if (pathname === '/' || pathname === '') return { name: 'home' };
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
  const item = ITEM.exec(pathname);
  if (item?.[1] !== undefined && item[2] !== undefined) {
    return { name: 'item', workspaceId: item[1], itemId: item[2] };
  }
  const itemView = ITEM_VIEW.exec(pathname);
  if (itemView?.[1] !== undefined && itemView[2] !== undefined && itemView[3] !== undefined) {
    return {
      name: 'item',
      workspaceId: itemView[1],
      itemId: itemView[2],
      view: taskView(itemView[3]),
    };
  }
  const itemTask = ITEM_TASK.exec(pathname);
  if (itemTask?.[1] !== undefined && itemTask[2] !== undefined && itemTask[3] !== undefined) {
    return {
      name: 'item',
      workspaceId: itemTask[1],
      itemId: itemTask[2],
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
  if (members?.[1] !== undefined) return { name: 'members', workspaceId: members[1] };
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

export function membersPath(workspaceId: string): string {
  return `/w/${workspaceId}/members`;
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

export function itemPath(workspaceId: string, itemId: string): string {
  return `/w/${workspaceId}/i/${itemId}`;
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
  path = $state(window.location.pathname);
  /** 주소의 `#` 부분(백링크가 원문 문서 안의 참조 자리를 가리킬 때). 경로 해석에는 쓰지 않는다. */
  hash = $state(window.location.hash);
  route = $derived(parseRoute(this.path));
  /**
   * 이동 전에 확인할 함수(미저장 변경 보호). false면 이동하지 않는다. 막은 뒤 확인 대화상자에서 떠나기로 하면 `proceed`로
   * 같은 이동을 다시 한다(그때는 가드가 true를 돌려줘야 한다).
   */
  guard: ((proceed: () => void) => boolean) | undefined = undefined;
  /** 가드가 막은 뒤로 가기를 떠나기로 확정했을 때 한 번 건너뛸 popstate(아래 참고). */
  private skipGuardOnce = false;

  constructor() {
    // 뒤로·앞으로 가기도 미저장 편집 보호를 거친다(D-121). 브라우저는 주소를 먼저 바꾸므로, 가드가 막으면 같은 기록을
    // 다시 쌓아 원래 주소로 되돌리고 화면은 그대로 둔다. 확인 대화상자에서 떠나기로 하면 `proceed`가 다시 뒤로 간다.
    window.addEventListener('popstate', () => {
      const next = window.location.pathname;
      const nextHash = window.location.hash;
      if (next !== this.path && this.guard !== undefined && !this.skipGuardOnce) {
        const guard = this.guard;
        const allowed = guard(() => {
          this.skipGuardOnce = true;
          window.history.back();
        });
        if (!allowed) {
          window.history.pushState(null, '', this.path + this.hash);
          return;
        }
      }
      this.skipGuardOnce = false;
      this.path = next;
      this.hash = nextHash;
    });
  }

  navigate(target: string): void {
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
      })
    )
      return;
    window.history.pushState(null, '', target);
    this.path = path;
    this.hash = hash;
  }

  /**
   * 이동이 아닌 문맥 전환(조직 바꾸기 등)도 같은 이탈 확인을 거친다(D-121·D-125). 가드가 없거나 허락하면 바로 실행하고,
   * 가드가 막으면 사용자가 떠나기로 했을 때 가드가 `action`을 실행한다. 취소하면 아무것도 바꾸지 않는다. `action` 안의
   * `navigate`는 그 확정 안에서 다시 묻지 않는다(가드가 proceed 동안 통과시킨다).
   */
  confirmLeave(action: () => void): void {
    if (this.guard !== undefined && !this.guard(action)) return;
    action();
  }

  /** 바로 앞 기록으로 돌아간다(이 화면이 기록을 쌓아 연 대화상자를 닫을 때). 주소는 popstate가 맞춘다. 가드가 막으면 false. */
  back(): boolean {
    if (
      this.guard !== undefined &&
      !this.guard(() => {
        this.back();
      })
    )
      return false;
    window.history.back();
    return true;
  }
}

export const router = new Router();
