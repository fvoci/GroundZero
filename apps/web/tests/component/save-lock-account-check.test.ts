import type { Me, MembersView, Module, Workspace } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api, type ItemView } from '$lib/api/client';
import { accountSafety, auth } from '$lib/features/feedback/feedback.svelte';
import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
import MembersScreen from '$lib/features/members/MembersScreen.svelte';
import ModuleScreen from '$lib/features/modules/ModuleScreen.svelte';
import ModulesSection from '$lib/features/modules/ModulesSection.svelte';
import { timeData } from '$lib/features/time/data';
import type { TimeEntryView } from '$lib/features/time/model';
import TimeEntryDialog from '$lib/features/time/TimeEntryDialog.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

// #313: 창으로 돌아올 때 FeedbackHost가 계정을 다시 확인한다(hideSession('reauth'), FeedbackHost.svelte:145). 그 사이 끝난
// 저장은 공통 request가 account_changed로 끝내고(shared/api/index.ts:111), 같은 사용자로 확인되면 화면을 다시 마운트하지
// 않고 가림만 걷는다(FeedbackHost.svelte:166-170). 그때 끝난 요청의 잠금이 남으면 폼을 다시 보낼 수 없고 앱 안 이동도
// 막힌다. 결과(입력 비우기·이동·오류 표시)는 그 요청이 확인 중에 끝났으므로 쓰지 않는다(D-96).

const workspace: Workspace = {
  id: 'ws-save-lock',
  organization_id: 'org-1',
  name: '공간',
  access: 'owner',
  role: 'admin',
  is_owner: true,
};

const viewer: Me = {
  id: 'save-lock-user',
  display_name: '사용자',
  is_server_admin: false,
  hour_cycle: '',
  locale: 'ko',
  time_zone: '',
  week_start: '',
  theme: 'system',
  limits: {
    max_title_chars: 200,
    max_document_bytes: 2_000_000,
    max_references_per_document: 500,
    max_document_depth: 10,
  },
};

const module: Module = {
  id: 'mod-check',
  workspace_id: workspace.id,
  kind: 'project',
  name: '모듈',
  key: 'CHK',
  previous_keys: [],
  archived: false,
  icon: '',
  description: '',
  access: 'owner',
};

const template: ItemView = {
  id: 'tpl-1',
  module_id: module.id,
  workspace_id: workspace.id,
  reference: 'CHK-1',
  title: '템플릿',
  description: '',
  visibility: 'workspace',
  access: 'owner',
  archived: false,
  updated_at: '',
  status: 'active',
  is_template: true,
  version: 1,
  starts_on: null,
  ends_on: null,
};

const members: MembersView = {
  can_manage_members: true,
  members: [
    {
      user_id: viewer.id,
      display_name: '사용자',
      email: 'viewer@lab.example',
      role: 'admin',
      is_owner: true,
    },
  ],
  my_role: 'admin',
  i_am_owner: true,
  pending_owner: null,
};

const entry: TimeEntryView = {
  id: 'entry-check',
  started_at: '2026-11-01T04:00:00Z',
  ended_at: '2026-11-01T06:00:00Z',
  paused_at: null,
  paused_seconds: 0,
  seconds: 7200,
  kind: 'work',
  memo: '저장한 메모',
  target: null,
  version: 3,
};

function held<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve: ((value: T) => void) | undefined;
  const promise = new Promise<T>((ok) => {
    resolve = ok;
  });
  if (resolve === undefined) throw new Error('promise');
  return { promise, resolve };
}

/** 쓰기 요청 하나만 붙잡고, 나머지 fetch는 끝나지 않게 둔다. */
function holdWrite(method: string, path: string): (response: Response) => void {
  const request = held<Response>();
  // 공통 request는 경로 문자열로 fetch를 부른다(shared/api/index.ts:102).
  vi.stubGlobal('fetch', (input: string, init?: RequestInit) =>
    init?.method === method && input === path
      ? request.promise
      : new Promise<Response>(() => undefined),
  );
  return request.resolve;
}

/**
 * 창으로 돌아왔더니 세션이 끝나 있었다(다른 탭의 로그아웃·만료): FeedbackHost가 화면을 가리고 다시 로그인을 기다린다
 * (FeedbackHost.svelte:162-164).
 */
async function sessionEndedOnReturn(): Promise<void> {
  vi.spyOn(api, 'me').mockResolvedValue(undefined);
  window.dispatchEvent(new Event('focus'));
  await waitFor(() => {
    expect(accountSafety.switching).toBe(true);
  });
  expect(accountSafety.reason).toBe('reauth');
}

/**
 * 붙잡은 응답을 푼 뒤 폼의 `finally`까지 돌게 한다. 응답 처리는 마이크로태스크만 이어지므로 다음 매크로태스크 전에
 * 모두 끝난다(가림이 걷히기 전에 끝난 요청이라는 조건을 고정한다).
 */
async function responseHandled(): Promise<void> {
  await new Promise<void>((done) => {
    setTimeout(done);
  });
  expect(accountSafety.switching).toBe(true);
}

/** 새 탭에서 같은 사용자로 다시 로그인하고 돌아왔다: 화면을 다시 마운트하지 않고 가림만 걷는다(:166-170). */
async function sameUserSignedInAgain(): Promise<void> {
  vi.mocked(api.me).mockResolvedValue(viewer);
  window.dispatchEvent(new Event('focus'));
  await waitFor(() => {
    expect(accountSafety.switching || accountSafety.restoring).toBe(false);
  });
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  localStorage.clear();
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
  Object.assign(me, { userId: viewer.id, timeZone: 'UTC', hourCycle: 'h23' });
  render(FeedbackHost, { reload: vi.fn<() => void>() });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  accountSafety.switching = false;
  accountSafety.restoring = false;
  accountSafety.error = '';
  auth.expired = false;
  Object.assign(me, { userId: '', timeZone: '', hourCycle: '' });
});

it('releases the module creation lock after a same-user check ends', async () => {
  const respond = holdWrite('POST', `/api/workspaces/${workspace.id}/modules`);
  const navigate = vi.spyOn(router, 'navigate').mockImplementation(() => undefined);
  render(ModulesSection, { workspace });
  await fireEvent.input(screen.getByLabelText(m.modules_name()), {
    target: { value: '확인 중 모듈' },
  });
  await fireEvent.input(screen.getByLabelText(m.modules_key()), { target: { value: 'CHK' } });
  await fireEvent.click(screen.getByRole('button', { name: m.modules_create() }));
  expect(screen.getByRole('button', { name: m.modules_create() })).toBeDisabled();

  await answerDuringCheck(respond, 201);

  await waitFor(() => {
    expect(screen.getByRole('button', { name: m.modules_create() })).not.toBeDisabled();
  });
  // 확인 중에 끝난 응답은 이 화면에 쓰지 않는다: 입력은 남고, 이동·오류도 없다(D-96).
  expect(screen.getByLabelText(m.modules_name())).toHaveValue('확인 중 모듈');
  expect(navigate).not.toHaveBeenCalled();
  expect(screen.queryByRole('alert')).toBeNull();
  await expectLeaveAsks();
});

it('releases the template copy lock after a same-user check ends', async () => {
  const respond = holdWrite('POST', '/api/items/tpl-1/copy');
  const navigate = vi.spyOn(router, 'navigate').mockImplementation(() => undefined);
  vi.spyOn(api, 'module').mockResolvedValue(module);
  vi.spyOn(api, 'items').mockResolvedValue([template]);
  render(ModuleScreen, { workspace, moduleId: module.id });
  const title = await screen.findByLabelText(m.modules_template_item_title());
  await fireEvent.input(title, { target: { value: '확인 중 복제' } });
  await fireEvent.click(screen.getByRole('button', { name: m.modules_from_template() }));
  expect(title).toBeDisabled();

  await answerDuringCheck(respond, 201);

  await waitFor(() => {
    expect(screen.getByLabelText(m.modules_template_item_title())).not.toBeDisabled();
  });
  expect(screen.getByLabelText(m.modules_template_item_title())).toHaveValue('확인 중 복제');
  expect(navigate).not.toHaveBeenCalled();
  expect(screen.queryByRole('alert')).toBeNull();
  await expectLeaveAsks();
});

it('releases the member invitation lock after a same-user check ends', async () => {
  const respond = holdWrite('POST', `/api/workspaces/${workspace.id}/invitations`);
  vi.spyOn(api, 'members').mockResolvedValue(members);
  vi.spyOn(api, 'groups').mockResolvedValue([]);
  vi.spyOn(api, 'invitations').mockResolvedValue([]);
  const changed = vi.fn<() => void>();
  render(MembersScreen, { workspace, tab: 'invitations', meId: viewer.id, onchanged: changed });
  const email = await screen.findByLabelText('이메일');
  await fireEvent.input(email, { target: { value: 'check@lab.example' } });
  await fireEvent.click(screen.getByRole('button', { name: m.members_invite() }));
  expect(email).toBeDisabled();

  await answerDuringCheck(respond, 201);

  await waitFor(() => {
    expect(screen.getByRole('button', { name: m.members_invite() })).not.toBeDisabled();
  });
  expect(screen.getByLabelText('이메일')).toHaveValue('check@lab.example');
  expect(changed).not.toHaveBeenCalled();
  expect(screen.queryByRole('alert')).toBeNull();
  await expectLeaveAsks();
});

it('releases the time entry save lock after a same-user check ends', async () => {
  const respond = holdWrite('PUT', `/api/time-entries/${entry.id}`);
  vi.spyOn(timeData, 'timeEntryRevisions').mockResolvedValue([]);
  const closed = vi.fn<() => void>();
  const changed = vi.fn<() => void>();
  render(TimeEntryDialog, { entry, zone: 'UTC', onclose: closed, onchanged: changed });
  const memo = await screen.findByLabelText(m.time_memo());
  await fireEvent.input(memo, { target: { value: '확인 중 메모' } });
  await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
  expect(memo).toBeDisabled();

  await answerDuringCheck(respond, 200);

  await waitFor(() => {
    expect(screen.getByRole('button', { name: m.common_save() })).not.toBeDisabled();
  });
  expect(screen.getByLabelText(m.time_memo())).toHaveValue('확인 중 메모');
  expect(closed).not.toHaveBeenCalled();
  expect(changed).not.toHaveBeenCalled();
  expect(screen.queryByRole('alert')).toBeNull();
  // 잠긴 대화상자는 닫기를 묻지 않고 무시한다. 풀리면 남은 메모 때문에 버리기 확인을 연다.
  await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
  expect(await screen.findByRole('alertdialog')).toBeInTheDocument();
  expect(closed).not.toHaveBeenCalled();
});

/**
 * 요청 중 세션이 끝난 것을 알고, 응답이 그 가림 안에서 끝난 뒤, 같은 사용자로 돌아온다. 응답 본문은 읽지 않는다: 공통
 * request가 안전 세대가 바뀐 것을 보고 account_changed로 끝낸다(shared/api/index.ts:111).
 */
async function answerDuringCheck(
  respond: (response: Response) => void,
  status: number,
): Promise<void> {
  await sessionEndedOnReturn();
  respond(new Response('{}', { status }));
  await responseHandled();
  await sameUserSignedInAgain();
}

/**
 * 잠긴 폼은 앱 안 이동을 묻지 않고 막는다(leave-guard.svelte.ts의 busy). 잠금이 풀리면 남은 입력 때문에 버리기 확인을
 * 연다.
 */
async function expectLeaveAsks(): Promise<void> {
  expect(router.guard?.(vi.fn(), '/')).toBe(false);
  expect(await screen.findByText(m.common_discard_title())).toBeInTheDocument();
}
