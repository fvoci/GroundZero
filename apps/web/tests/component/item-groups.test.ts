import './fixtures/task-statuses';
import type { Module, ModuleKind, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
import ModuleScreen from '$lib/features/modules/ModuleScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { org } from '$lib/org.svelte';
import { me } from '$lib/me.svelte';
import { itemPath, itemViewPath, modulePath, router } from '$lib/router.svelte';
import { scopedKey } from '$lib/storage';
import './fixtures/authenticated';

// 항목 화면의 묶음(D-121, 사용자 R7 A): 종류별 첫 묶음 + 공통 업무·문서·일정·기록. 묶음은 하위 경로이고 경로 없이 열면
// 세션 기억 › 종류별 기본 묶음으로 replaceState, 전환은 pushState, 고른 묶음만 마운트한다. 언마운트되는 묶음의 편집은
// D-125 확인을 거치고, 묶음과 상관없이 마운트된 머리 정보 폼은 묶음을 바꿔도 묻지 않는다(코디네이터 지시 2026-10-27).
// 모듈 화면은 항목·달력 두 보기이고 달력은 고른 때만 조회한다.

const ws = '11111111-1111-1111-1111-111111111111';
const itemId = '22222222-2222-2222-2222-222222222222';
const otherItemId = '55555555-5555-5555-5555-555555555555';
const moduleId = '44444444-4444-4444-4444-444444444444';

const workspace: Workspace = {
  access: 'owner',
  id: ws,
  organization_id: 'org-1',
  name: '공간',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};

function moduleOf(kind: ModuleKind): Module {
  return {
    access: 'owner',
    id: moduleId,
    workspace_id: ws,
    kind,
    name: '개발',
    key: 'DEV',
    previous_keys: [],
    archived: false,
    icon: '',
    description: '',
  };
}

function itemOf(id: string): ItemView {
  return {
    id,
    module_id: moduleId,
    workspace_id: ws,
    reference: 'DEV-1',
    title: `항목 ${id.slice(0, 4)}`,
    description: '',
    visibility: 'workspace',
    access: 'owner',
    archived: false,
    updated_at: '2026-10-19T00:00:00Z',
    status: 'active',
    is_template: false,
    version: 1,
    starts_on: null,
    ends_on: null,
  };
}

/** 묶음 안 패널의 나머지 요청은 끝나지 않게 둔다. 이 시험은 어느 묶음이 마운트되는지만 본다. */
function server(kind: ModuleKind): void {
  vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
  vi.spyOn(api, 'item').mockImplementation((id: string) => Promise.resolve(itemOf(id)));
  vi.spyOn(api, 'module').mockResolvedValue(moduleOf(kind));
  vi.spyOn(api, 'itemDocuments').mockResolvedValue([]);
}

function goTo(path: string): void {
  window.history.replaceState(null, '', path);
  router.path = path;
  router.hash = '';
}

function popTo(path: string): void {
  window.history.replaceState(null, '', path);
  window.dispatchEvent(new PopStateEvent('popstate'));
}

const groups = (): HTMLElement => screen.getByRole('navigation', { name: '항목 묶음' });
const groupLink = (name: string): HTMLElement => within(groups()).getByRole('link', { name });
const mounted = (id: string): boolean => document.getElementById(id) !== null;
const groupKey = (): string => scopedKey(`item-group:${itemId}`, 'user-a', 'org-1');

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  sessionStorage.clear();
  router.guard = undefined;
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  sessionStorage.clear();
  router.guard = undefined;
  goTo('/');
});

async function openItem(kind: ModuleKind, path = itemPath(ws, itemId)): Promise<void> {
  server(kind);
  goTo(path);
  render(ItemScreen, { props: { workspace, itemId, workspaces: [workspace] } });
  await screen.findByRole('navigation', { name: '항목 묶음' });
}

describe('item screen groups', () => {
  it('replaces the root path with the project default tasks/board and mounts only the tasks group', async () => {
    const before = window.history.length;
    await openItem('project');

    expect(window.location.pathname).toBe(itemViewPath(ws, itemId, 'board'));
    expect(window.history.length).toBe(before);
    expect(groupLink('업무')).toHaveAttribute('aria-current', 'page');
    expect(
      within(groups())
        .getAllByRole('link')
        .map((link) => link.textContent.trim()),
    ).toEqual(['업무', '문서', '일정', '기록']);
    expect(mounted('tasks-heading')).toBe(true);
    expect(mounted('item-docs-heading')).toBe(false);
    expect(mounted('events-heading')).toBe(false);
    expect(mounted('attachments-heading')).toBe(false);
  });

  it.each([
    ['research', 'references', '문헌'],
    ['study', 'plan', '계획'],
    ['course', 'episodes', '회차'],
    ['hobby', 'activities', '활동'],
  ] as const)('opens a %s item on its first group %s', async (kind, group, label) => {
    await openItem(kind);

    expect(window.location.pathname).toBe(`${itemPath(ws, itemId)}/${group}`);
    const links = within(groups()).getAllByRole('link');
    expect(links.map((link) => link.textContent.trim())).toEqual([
      label,
      '업무',
      '문서',
      '일정',
      '기록',
    ]);
    expect(groupLink(label)).toHaveAttribute('aria-current', 'page');
    expect(mounted('tasks-heading')).toBe(false);
    expect(mounted('item-docs-heading')).toBe(false);
  });

  it('reopens the root path on the group remembered for this user and organization', async () => {
    sessionStorage.setItem(groupKey(), JSON.stringify({ group: 'events', view: 'board' }));
    await openItem('project');

    expect(window.location.pathname).toBe(`${itemPath(ws, itemId)}/events`);
    expect(mounted('events-heading')).toBe(true);
    expect(mounted('tasks-heading')).toBe(false);
  });

  it('ignores a remembered group that the module kind does not have', async () => {
    sessionStorage.setItem(groupKey(), JSON.stringify({ group: 'plan', view: 'board' }));
    await openItem('project');

    expect(window.location.pathname).toBe(itemViewPath(ws, itemId, 'board'));
  });

  it('replaces a group path of another kind with the kind default', async () => {
    await openItem('project', `${itemPath(ws, itemId)}/references`);

    expect(window.location.pathname).toBe(itemViewPath(ws, itemId, 'board'));
  });

  it('switches groups with pushState, mounts only the chosen one and remembers it', async () => {
    await openItem('project');
    const before = window.history.length;

    await fireEvent.click(groupLink('문서'));

    expect(window.location.pathname).toBe(`${itemPath(ws, itemId)}/docs`);
    expect(window.history.length).toBe(before + 1);
    expect(groupLink('문서')).toHaveAttribute('aria-current', 'page');
    expect(groupLink('업무')).not.toHaveAttribute('aria-current');
    expect(mounted('item-docs-heading')).toBe(true);
    expect(mounted('tasks-heading')).toBe(false);
    expect(JSON.parse(sessionStorage.getItem(groupKey()) ?? 'null')).toMatchObject({
      group: 'docs',
    });

    await fireEvent.click(groupLink('기록'));
    expect(window.location.pathname).toBe(`${itemPath(ws, itemId)}/records`);
    expect(mounted('attachments-heading')).toBe(true);
    expect(mounted('item-docs-heading')).toBe(false);
  });

  it('returns to the last task view when the tasks group is chosen again', async () => {
    await openItem('project', itemViewPath(ws, itemId, 'gantt'));
    await fireEvent.click(groupLink('문서'));
    await fireEvent.click(groupLink('업무'));

    expect(window.location.pathname).toBe(itemViewPath(ws, itemId, 'gantt'));
  });

  it('follows the address when popstate changes the group', async () => {
    await openItem('project', `${itemPath(ws, itemId)}/docs`);

    popTo(`${itemPath(ws, itemId)}/events`);

    await waitFor(() => {
      expect(mounted('events-heading')).toBe(true);
    });
    expect(mounted('item-docs-heading')).toBe(false);
  });
});

describe('item screen groups and unsaved input', () => {
  async function typeDocTitle(value: string): Promise<HTMLInputElement> {
    const input = screen.getByLabelText<HTMLInputElement>('새 문서 제목');
    await fireEvent.input(input, { target: { value } });
    return input;
  }

  it('keeps the header form mounted and unasked when only the group changes', async () => {
    await openItem('project', `${itemPath(ws, itemId)}/docs`);
    const title = screen.getByLabelText<HTMLInputElement>('제목');
    await fireEvent.input(title, { target: { value: '고친 제목' } });

    await fireEvent.click(groupLink('일정'));

    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(window.location.pathname).toBe(`${itemPath(ws, itemId)}/events`);
    expect(screen.getByLabelText<HTMLInputElement>('제목').value).toBe('고친 제목');

    // 같은 항목 안의 업무 보기 전환도 같은 규칙이다.
    await fireEvent.click(groupLink('업무'));
    await fireEvent.click(screen.getByRole('button', { name: '목록' }));
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(window.location.pathname).toBe(itemViewPath(ws, itemId, 'list'));
    expect(screen.getByLabelText<HTMLInputElement>('제목').value).toBe('고친 제목');
  });

  it('asks before unmounting a group with unsaved input and keeps it on cancel', async () => {
    await openItem('project', `${itemPath(ws, itemId)}/docs`);
    await typeDocTitle('새 문서');

    await fireEvent.click(groupLink('일정'));
    const dialog = await screen.findByRole('alertdialog');
    await fireEvent.click(within(dialog).getByRole('button', { name: '계속 편집' }));

    await waitFor(() => {
      expect(screen.queryByRole('alertdialog')).toBeNull();
    });
    expect(window.location.pathname).toBe(`${itemPath(ws, itemId)}/docs`);
    expect(screen.getByLabelText<HTMLInputElement>('새 문서 제목').value).toBe('새 문서');
    expect(mounted('events-heading')).toBe(false);
  });

  it('discards only the leaving group input and keeps the header input', async () => {
    await openItem('project', `${itemPath(ws, itemId)}/docs`);
    const title = screen.getByLabelText<HTMLInputElement>('제목');
    await fireEvent.input(title, { target: { value: '고친 제목' } });
    await typeDocTitle('새 문서');

    await fireEvent.click(groupLink('일정'));
    const dialog = await screen.findByRole('alertdialog');
    await fireEvent.click(within(dialog).getByRole('button', { name: '변경 버리고 닫기' }));

    await waitFor(() => {
      expect(mounted('events-heading')).toBe(true);
    });
    expect(window.location.pathname).toBe(`${itemPath(ws, itemId)}/events`);
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(screen.getByLabelText<HTMLInputElement>('제목').value).toBe('고친 제목');

    await fireEvent.click(groupLink('문서'));
    expect(screen.getByLabelText<HTMLInputElement>('새 문서 제목').value).toBe('');
    expect(screen.queryByTestId('form-draft-offer')).toBeNull();
  });

  it('asks about the header input when leaving for another item', async () => {
    await openItem('project', `${itemPath(ws, itemId)}/docs`);
    const title = screen.getByLabelText<HTMLInputElement>('제목');
    await fireEvent.input(title, { target: { value: '고친 제목' } });

    router.navigate(itemPath(ws, otherItemId));

    expect(await screen.findByRole('alertdialog')).toBeInTheDocument();
    expect(window.location.pathname).toBe(`${itemPath(ws, itemId)}/docs`);
  });

  it('asks on popstate away from a group with unsaved input', async () => {
    await openItem('project', `${itemPath(ws, itemId)}/docs`);
    await typeDocTitle('새 문서');

    popTo(`${itemPath(ws, itemId)}/events`);

    expect(await screen.findByRole('alertdialog')).toBeInTheDocument();
    expect(window.location.pathname).toBe(`${itemPath(ws, itemId)}/docs`);
    expect(mounted('item-docs-heading')).toBe(true);
    expect(mounted('events-heading')).toBe(false);
  });

  it('lets popstate within the item through for the header input', async () => {
    await openItem('project', `${itemPath(ws, itemId)}/docs`);
    const title = screen.getByLabelText<HTMLInputElement>('제목');
    await fireEvent.input(title, { target: { value: '고친 제목' } });

    popTo(`${itemPath(ws, itemId)}/events`);

    await waitFor(() => {
      expect(mounted('events-heading')).toBe(true);
    });
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(screen.getByLabelText<HTMLInputElement>('제목').value).toBe('고친 제목');
  });
});

// 연구 문헌·취미 활동은 기록 종류다. 첫 묶음에는 그 종류만, 공통 기록 묶음에는 나머지 종류가 보인다.
describe('record kinds split between the first group and the records group', () => {
  function kind(name: string, label: string, moduleKind: ModuleKind) {
    return {
      kind: name,
      label,
      module_kind: moduleKind,
      date_label: null,
      fields: [],
      file_roles: [],
    };
  }

  it.each([
    ['research', '문헌', 'reference', '문헌', 'dataset', '데이터'],
    ['hobby', '활동', 'activity', '활동', 'supply', '재료'],
  ] as const)(
    'shows only the %s first-group kind there and the other kinds under records',
    async (moduleKind, groupLabel, firstKind, firstLabel, otherKind, otherLabel) => {
      vi.spyOn(api, 'recordKinds').mockResolvedValue([
        kind(firstKind, firstLabel, moduleKind),
        kind(otherKind, otherLabel, moduleKind),
        kind('risk', '위험', 'project'),
      ]);
      vi.spyOn(api, 'itemRecords').mockResolvedValue([]);
      vi.spyOn(api, 'members').mockResolvedValue({
        members: [],
        my_role: 'member',
        can_manage_members: false,
        i_am_owner: false,
        pending_owner: null,
      });
      await openItem(moduleKind);
      expect(groupLink(groupLabel)).toHaveAttribute('aria-current', 'page');

      const first = await screen.findByRole('group', { name: '기록 종류' });
      expect(within(first).getAllByRole('button')).toHaveLength(1);
      expect(first).toHaveTextContent(firstLabel);

      await fireEvent.click(groupLink('기록'));
      await waitFor(() => {
        expect(screen.getByRole('group', { name: '기록 종류' })).toHaveTextContent(otherLabel);
      });
      const rest = screen.getByRole('group', { name: '기록 종류' });
      expect(within(rest).getAllByRole('button')).toHaveLength(1);
      expect(rest).not.toHaveTextContent(firstLabel);
    },
  );
});

describe('module screen views', () => {
  function moduleServer() {
    vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
    vi.spyOn(api, 'module').mockResolvedValue(moduleOf('project'));
    vi.spyOn(api, 'items').mockResolvedValue([itemOf(itemId)]);
    return vi.spyOn(api, 'calendar').mockReturnValue(new Promise(() => undefined));
  }

  it('shows the items view first and queries the calendar only after it is chosen', async () => {
    const calendar = moduleServer();
    goTo(modulePath(ws, moduleId));
    render(ModuleScreen, { props: { workspace, moduleId } });
    const views = await screen.findByRole('navigation', { name: '모듈 보기' });

    expect(within(views).getByRole('link', { name: '항목' })).toHaveAttribute(
      'aria-current',
      'page',
    );
    expect(screen.getByTestId('item-list')).toBeInTheDocument();
    expect(calendar).not.toHaveBeenCalled();
    const before = window.history.length;

    await fireEvent.click(within(views).getByRole('link', { name: '달력' }));

    expect(window.location.pathname).toBe(`${modulePath(ws, moduleId)}/calendar`);
    expect(window.history.length).toBe(before + 1);
    await waitFor(() => {
      expect(calendar).toHaveBeenCalled();
    });
    expect(screen.queryByTestId('item-list')).toBeNull();
  });

  it('opens the calendar view from its path', async () => {
    const calendar = moduleServer();
    goTo(`${modulePath(ws, moduleId)}/calendar`);
    render(ModuleScreen, { props: { workspace, moduleId } });
    await screen.findByRole('navigation', { name: '모듈 보기' });

    await waitFor(() => {
      expect(calendar).toHaveBeenCalled();
    });
    expect(screen.queryByTestId('item-list')).toBeNull();
  });
});

// 다른 사용자의 기억은 읽지 않는다(D-121 저장소 키에 사용자·조직).
describe('item group memory scope', () => {
  it('does not read another user group memory', async () => {
    sessionStorage.setItem(
      scopedKey(`item-group:${itemId}`, 'user-b', 'org-1'),
      JSON.stringify({ group: 'events', view: 'board' }),
    );
    expect(me.userId).toBe('user-a');
    expect(org.currentId).toBe('org-1');
    await openItem('project');

    expect(window.location.pathname).toBe(itemViewPath(ws, itemId, 'board'));
  });
});
