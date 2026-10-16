// 최소 경로 라우터. SvelteKit을 쓰지 않으므로(D-66) history API로 경로만 관리한다.
// 서버는 알 수 없는 경로에 index.html을 돌려 새로고침을 지원한다.

export type Route =
  | { name: 'home' }
  | { name: 'workspace'; workspaceId: string }
  | { name: 'document'; workspaceId: string; documentId: string }
  | { name: 'members'; workspaceId: string }
  | { name: 'not-found' };

const UUID = '[0-9a-fA-F-]{36}';
const WORKSPACE = new RegExp(`^/w/(${UUID})/?$`);
const DOCUMENT = new RegExp(`^/w/(${UUID})/wiki/(${UUID})/?$`);
const MEMBERS = new RegExp(`^/w/(${UUID})/members/?$`);

export function parseRoute(pathname: string): Route {
  if (pathname === '/' || pathname === '') return { name: 'home' };
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

export function documentPath(workspaceId: string, documentId: string): string {
  return `/w/${workspaceId}/wiki/${documentId}`;
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
