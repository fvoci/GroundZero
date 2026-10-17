// 최소 경로 라우터. SvelteKit을 쓰지 않으므로(D-66) history API로 경로만 관리한다.
// 서버는 알 수 없는 경로에 index.html을 돌려 새로고침을 지원한다.

export type Route =
  | { name: 'home' }
  | { name: 'workspace'; workspaceId: string }
  | { name: 'document'; workspaceId: string; documentId: string }
  | { name: 'members'; workspaceId: string }
  | { name: 'calendar'; workspaceId: string }
  | { name: 'trash'; workspaceId: string }
  | { name: 'my-trash' }
  | { name: 'module'; workspaceId: string; moduleId: string }
  | { name: 'item'; workspaceId: string; itemId: string; taskId?: string }
  | { name: 'item-document'; workspaceId: string; itemId: string; documentId: string }
  | { name: 'notes' }
  | { name: 'shared' }
  | { name: 'notifications' }
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
const STANDALONE = new RegExp(`^/d/(${UUID})/?$`);
const MODULE = new RegExp(`^/w/(${UUID})/m/(${UUID})/?$`);
const ITEM = new RegExp(`^/w/(${UUID})/i/(${UUID})/?$`);
const ITEM_TASK = new RegExp(`^/w/(${UUID})/i/(${UUID})/t/(${UUID})/?$`);
const ITEM_DOCUMENT = new RegExp(`^/w/(${UUID})/i/(${UUID})/d/(${UUID})/?$`);

export function parseRoute(pathname: string): Route {
  if (pathname === '/' || pathname === '') return { name: 'home' };
  const pub = PUBLIC.exec(pathname);
  if (pub?.[1] !== undefined) return { name: 'public', token: pub[1] };
  if (pathname === '/notes' || pathname === '/notes/') return { name: 'notes' };
  if (pathname === '/shared' || pathname === '/shared/') return { name: 'shared' };
  if (pathname === '/time' || pathname === '/time/') return { name: 'time' };
  if (pathname === '/trash' || pathname === '/trash/') return { name: 'my-trash' };
  if (pathname === '/notifications' || pathname === '/notifications/')
    return { name: 'notifications' };
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
  const itemTask = ITEM_TASK.exec(pathname);
  if (itemTask?.[1] !== undefined && itemTask[2] !== undefined && itemTask[3] !== undefined) {
    return { name: 'item', workspaceId: itemTask[1], itemId: itemTask[2], taskId: itemTask[3] };
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

export function itemTaskPath(workspaceId: string, itemId: string, taskId: string): string {
  return `/w/${workspaceId}/i/${itemId}/t/${taskId}`;
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
  route = $derived(parseRoute(this.path));
  /** 이동 전에 확인할 함수(미저장 변경 보호). false면 이동하지 않는다. */
  guard: (() => boolean) | undefined = undefined;

  constructor() {
    window.addEventListener('popstate', () => {
      this.path = window.location.pathname;
    });
  }

  navigate(path: string): void {
    if (path === this.path) return;
    if (this.guard !== undefined && !this.guard()) return;
    window.history.pushState(null, '', path);
    this.path = path;
  }
}

export const router = new Router();
