import { timeData } from '$lib/features/time/data';
import { taskData as work } from '$lib/features/tasks/detail-data';
// 화면 단위 story(Screens/)의 준비: 경로(router.path), 사이드바가 같이 쓰는 목록(workspaceNav), 서버 응답(`api` 함수)과
// 빌드 정보(/healthz)를 story 동안만 바꾼다. 실제 서버에 닿는 요청은 하나도 보내지 않는다(preview.ts의 요청 차단).
import { spyOn } from 'storybook/test';

import type { TaskView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import { applyMove } from '$lib/features/tasks/board';
import { workspaceNav } from '$lib/features/nav/workspace-nav.svelte';
import { org } from '$lib/org.svelte';
import { router } from '$lib/router.svelte';
// 개인 화면·활동 story의 실제 계약 모양을 채운다.
import { ACTIVITY, myTasksView } from './personal';

import {
  calendarView,
  DOCS,
  homeView,
  itemsOf,
  membersView,
  MODULES,
  notificationsView,
  ORGANIZATION,
  TASK_ITEM,
  taskViews,
  WORKSPACE,
} from './synthetic';

/** 화면이 부르는 서버 함수. 기본값은 합성 데이터로 답하고, story가 일부만 바꾼다. */
export interface ScreenApi {
  myTasks: typeof api.myTasks;
  recentActivity: typeof api.recentActivity;
  modules: typeof api.modules;
  wikiDocuments: typeof api.wikiDocuments;
  module: typeof api.module;
  items: typeof api.items;
  home: typeof api.home;
  calendar: typeof api.calendar;
  notifications: typeof api.notifications;
  myInvitations: typeof api.myInvitations;
  currentTimer: typeof timeData.currentTimer;
  // 업무 보기는 서버 상태 정의를 함께 읽는다.
  taskStatuses: typeof work.taskStatuses;
  tasks: typeof work.tasks;
  savedViews: typeof api.savedViews;
  syncRecurrences: typeof work.syncRecurrences;
  task: typeof work.task;
  moveTask: typeof work.moveTask;
  members: typeof api.members;
  assignable: typeof work.assignable;
  search: typeof api.search;
  myOrganizations: typeof api.myOrganizations;
  unreadCount: typeof api.unreadCount;
}

/** 응답이 오지 않는 요청(불러오는 중). */
export function never<T>(): Promise<T> {
  return new Promise<T>(() => undefined);
}

/**
 * 업무 항목의 업무 목록. story마다 새로 만들고, 기본 옮기기는 서버처럼 목록에 반영하고 이동 판을 올린다(자리 규칙은
 * 화면과 같은 `applyMove`). 옮기기를 바꿔 끼운 story에서는 목록이 그대로다(서버가 아무것도 바꾸지 않음).
 */
function taskServer(): Pick<ScreenApi, 'tasks' | 'task' | 'moveTask'> {
  let list: TaskView[] = taskViews();
  const find = (id: string): TaskView => {
    const found = list.find((t) => t.id === id);
    if (found === undefined) throw new ApiError(404, 'not_found', 'task not found');
    return found;
  };
  return {
    tasks: (itemId) => Promise.resolve(itemId === TASK_ITEM.id ? list : []),
    task: (id) => Promise.resolve({ ...find(id), checklist: [] }),
    moveTask: (_expectedUser, id, status, beforeId) => {
      const before = find(id);
      list = applyMove(list, id, { status, beforeId }).map((t) =>
        t.id === id ? { ...t, move_version: before.move_version + 1 } : t,
      );
      return Promise.resolve({ ...find(id), checklist: [] });
    },
  };
}

const DEFAULTS: Omit<ScreenApi, 'tasks' | 'task' | 'moveTask'> = {
  // ⑪ 기본 상태 정의(이름은 그대로 보이고 범주로만 완료를 판단한다). 합성 업무(synthetic.ts)의 상태 이름과 같다.
  taskStatuses: () =>
    Promise.resolve([
      { id: 'todo', name: '할 일', color: '', position: 0, category: 'todo', task_count: 4 },
      {
        id: 'in_progress',
        name: '진행 중',
        color: '',
        position: 1,
        category: 'doing',
        task_count: 2,
      },
      { id: 'done', name: '완료', color: '', position: 2, category: 'done', task_count: 1 },
    ]),
  myTasks: () => Promise.resolve(myTasksView()),
  recentActivity: () => Promise.resolve(ACTIVITY),
  modules: () => Promise.resolve(MODULES),
  wikiDocuments: () => Promise.resolve(DOCS),
  module: (id) => {
    const found = MODULES.find((mod) => mod.id === id);
    return found === undefined
      ? Promise.reject(new ApiError(404, 'not_found', 'module not found'))
      : Promise.resolve(found);
  },
  items: (moduleId) => {
    const found = MODULES.find((mod) => mod.id === moduleId);
    return Promise.resolve(found === undefined ? [] : itemsOf(found));
  },
  home: () => Promise.resolve(homeView()),
  calendar: () => Promise.resolve(calendarView()),
  notifications: () => Promise.resolve({ notifications: notificationsView(), next: null }),
  myInvitations: () => Promise.resolve([]),
  currentTimer: () => Promise.resolve(null),
  savedViews: () => Promise.resolve([]),
  syncRecurrences: () => Promise.resolve(0),
  members: () => Promise.resolve(membersView()),
  assignable: () =>
    Promise.resolve(
      membersView().members.map((p) => ({ user_id: p.user_id, display_name: p.display_name })),
    ),
  search: () => Promise.resolve([]),
  myOrganizations: () => Promise.resolve([ORGANIZATION]),
  unreadCount: () => Promise.resolve(notificationsView().filter((n) => !n.read).length),
};

/** 계정 메뉴가 묻는 빌드 정보(/healthz). 개발 빌드로 답한다. */
const HEALTH = { status: 'ok', database: 'ok', build: { version: 'dev', source: 'storybook' } };

/**
 * 경로를 정하고 서버 함수를 바꿔 끼운다. story의 `beforeEach`에서 부르고 돌려준 정리 함수를 그대로 돌려준다(경로와
 * fetch를 되돌린다). 바꿔 끼운 `api` 함수는 Storybook이 story마다 원래대로 돌린다.
 */
export function setUpScreen(path: string, overrides: Partial<ScreenApi> = {}): () => void {
  // 앞 story에서 열어 둔 메뉴·서랍의 body 잠금(pointer-events)은 그 story가 사라진 뒤에야 풀린다. Storybook에서 story를
  // 바로 바꾸면 이 story의 클릭이 막히므로 먼저 푼다(tests/component/stories.test.ts의 afterEach와 같은 정리).
  document.body.style.removeProperty('pointer-events');
  const stubs: ScreenApi = { ...DEFAULTS, ...taskServer(), ...overrides };
  spyOn(api, 'myTasks').mockImplementation(stubs.myTasks);
  spyOn(api, 'recentActivity').mockImplementation(stubs.recentActivity);
  spyOn(api, 'modules').mockImplementation(stubs.modules);
  spyOn(api, 'wikiDocuments').mockImplementation(stubs.wikiDocuments);
  spyOn(api, 'module').mockImplementation(stubs.module);
  spyOn(api, 'items').mockImplementation(stubs.items);
  spyOn(api, 'home').mockImplementation(stubs.home);
  spyOn(api, 'calendar').mockImplementation(stubs.calendar);
  spyOn(api, 'notifications').mockImplementation(stubs.notifications);
  spyOn(api, 'myInvitations').mockImplementation(stubs.myInvitations);
  spyOn(timeData, 'currentTimer').mockImplementation(stubs.currentTimer);
  spyOn(work, 'tasks').mockImplementation(stubs.tasks);
  // 실제 상태 정의 조회도 합성 응답으로 격리한다.
  spyOn(work, 'taskStatuses').mockImplementation(stubs.taskStatuses);
  spyOn(api, 'savedViews').mockImplementation(stubs.savedViews);
  spyOn(work, 'syncRecurrences').mockImplementation(stubs.syncRecurrences);
  spyOn(work, 'task').mockImplementation(stubs.task);
  spyOn(work, 'moveTask').mockImplementation(stubs.moveTask);
  spyOn(api, 'members').mockImplementation(stubs.members);
  spyOn(work, 'assignable').mockImplementation(stubs.assignable);
  spyOn(api, 'search').mockImplementation(stubs.search);
  spyOn(api, 'myOrganizations').mockImplementation(stubs.myOrganizations);
  spyOn(api, 'unreadCount').mockImplementation(stubs.unreadCount);

  // 개인 화면은 현재 조직 범위다(D-111). story는 합성 조직 하나를 고른 상태로 시작한다.
  org.load('story-user', [ORGANIZATION], ORGANIZATION.id);

  // 사이드바 목록은 앱 전체가 같이 쓰는 저장소다. 같은 워크스페이스라도 앞 story의 목록·오류를 이어 받지 않게 비운다.
  workspaceNav.workspaceId = undefined;
  workspaceNav.modules = { state: 'loading' };
  workspaceNav.docs = { state: 'loading' };
  workspaceNav.currentItem = undefined;
  // 마지막으로 연 워크스페이스(사이드바가 브라우저에 남기는 편의 기억)를 정해 두어 보는 브라우저마다 다르지 않게 한다.
  try {
    // 마지막으로 연 워크스페이스 기억은 사용자·조직별 키다(D-121).
    window.localStorage.setItem(org.key('lastWorkspace'), WORKSPACE.id);
  } catch {
    // 저장소를 못 쓰면 사이드바도 기억하지 않는다.
  }

  // fetchHealth는 `api`가 아니라 모듈 함수라 바꿔 끼울 수 없다. /healthz만 합성 응답을 주고 나머지는 앞의 차단으로 넘긴다.
  const guarded = globalThis.fetch;
  globalThis.fetch = (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const url = new URL(input instanceof Request ? input.url : input, window.location.href);
    if (url.origin === window.location.origin && url.pathname === '/healthz') {
      return Promise.resolve(
        new Response(JSON.stringify(HEALTH), { headers: { 'content-type': 'application/json' } }),
      );
    }
    return guarded(input, init);
  };

  // 실제 주소(iframe.html)는 바꾸지 않고 앱 라우터가 보는 경로만 바꾼다.
  const before = window.location.pathname;
  router.path = path;
  return () => {
    globalThis.fetch = guarded;
    router.path = before;
  };
}
