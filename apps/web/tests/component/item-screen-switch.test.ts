import type { Module, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ItemProfileView, ItemView, SummaryView } from '$lib/api/client';
import { api } from '$lib/api/client';
import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const workspace: Workspace = {
  id: 'ws-1',
  name: '공간',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};

const mod: Module = {
  id: 'm1',
  workspace_id: 'ws-1',
  kind: 'project',
  name: '개발',
  key: 'DEV',
  previous_keys: [],
  archived: false,
  icon: '',
  description: '',
};

function item(id: 'A' | 'B'): ItemView {
  return {
    id,
    module_id: 'm1',
    workspace_id: 'ws-1',
    reference: `DEV-${id}`,
    title: `항목 ${id}`,
    description: '',
    visibility: 'workspace',
    access: 'owner',
    archived: false,
    updated_at: '2026-10-19T00:00:00Z',
    status: 'active',
    is_template: false,
    starts_on: null,
    ends_on: null,
  };
}

const profile: ItemProfileView = {
  leader: null,
  goal: '',
  scope: '',
  starts_on: null,
  ends_on: null,
  status: 'active',
  details: {},
  members: [],
  groups: [],
  can_edit: true,
  can_manage_members: false,
};

/** 손으로 끝내는 응답. */
function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve: (value: T) => void = () => undefined;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

/** 하위 칸(업무·일정·기록 등)이 부르는 나머지 요청은 끝나지 않게 둔다. 이 테스트는 항목 자체만 본다. */
function quietServer(): void {
  vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
  vi.spyOn(api, 'module').mockResolvedValue(mod);
  vi.spyOn(api, 'itemDocuments').mockResolvedValue([]);
}

/** 남은 응답과 화면 갱신이 끝나기를 기다린다. */
async function settle(): Promise<void> {
  await new Promise((r) => setTimeout(r, 0));
  await tick();
}

const heading = (): HTMLElement => screen.getByRole('heading', { level: 1 });

// 감사 W18: 항목 화면은 다른 항목으로 옮겨 가도 다시 만들어지지 않는다. A에서 보낸 저장·보관·템플릿 표시·정보
// 저장 뒤 다시 읽기의 응답이 B를 다 읽은 뒤 오면 B 주소에 A가 들어앉았다. 시작한 항목이 아직 화면의 항목일 때만
// 결과를 넣는다. A의 "저장했습니다."·오류도 B에 남지 않는다.
describe('item screen results after switching items', () => {
  it('drops a late save of the previous item', async () => {
    await setLocale('ko', { reload: false });
    quietServer();
    vi.spyOn(api, 'item').mockImplementation((id: string) =>
      Promise.resolve(item(id === 'A' ? 'A' : 'B')),
    );
    const write = deferred<ItemView>();
    vi.spyOn(api, 'updateItem').mockReturnValue(write.promise);
    const view = render(ItemScreen, { props: { workspace, itemId: 'A', workspaces: [workspace] } });
    await screen.findByRole('heading', { name: '항목 A' });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));

    await view.rerender({ workspace, itemId: 'B', workspaces: [workspace] });
    await screen.findByRole('heading', { name: '항목 B' });
    write.resolve(item('A'));
    await settle();

    expect(heading()).toHaveTextContent('항목 B');
    expect(screen.queryByText('저장했습니다.')).toBeNull();
  });

  it('drops a late archive of the previous item', async () => {
    await setLocale('ko', { reload: false });
    quietServer();
    vi.spyOn(api, 'item').mockImplementation((id: string) =>
      Promise.resolve(item(id === 'A' ? 'A' : 'B')),
    );
    const write = deferred<ItemView>();
    vi.spyOn(api, 'archiveItem').mockReturnValue(write.promise);
    const view = render(ItemScreen, { props: { workspace, itemId: 'A', workspaces: [workspace] } });
    await screen.findByRole('heading', { name: '항목 A' });
    await fireEvent.click(screen.getByRole('button', { name: '보관' }));

    await view.rerender({ workspace, itemId: 'B', workspaces: [workspace] });
    await screen.findByRole('heading', { name: '항목 B' });
    write.resolve({ ...item('A'), archived: true });
    await settle();

    expect(heading()).toHaveTextContent('항목 B');
  });

  it('drops a late re-read after the previous item profile was saved', async () => {
    await setLocale('ko', { reload: false });
    quietServer();
    const reread = deferred<ItemView>();
    let readsOfA = 0;
    vi.spyOn(api, 'item').mockImplementation((id: string) => {
      if (id !== 'A') return Promise.resolve(item('B'));
      readsOfA += 1;
      return readsOfA === 1 ? Promise.resolve(item('A')) : reread.promise;
    });
    vi.spyOn(api, 'itemProfile').mockResolvedValue(profile);
    vi.spyOn(api, 'setItemProfile').mockResolvedValue(undefined);
    const view = render(ItemScreen, { props: { workspace, itemId: 'A', workspaces: [workspace] } });
    const panel = await screen.findByRole('region', { name: '정보' });
    await fireEvent.click(await within(panel).findByRole('button', { name: '고치기' }));
    await fireEvent.click(within(panel).getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(readsOfA).toBe(2);
    });

    await view.rerender({ workspace, itemId: 'B', workspaces: [workspace] });
    await screen.findByRole('heading', { name: '항목 B' });
    reread.resolve(item('A'));
    await settle();

    expect(heading()).toHaveTextContent('항목 B');
  });

  it('does not carry the previous item "saved" message over', async () => {
    await setLocale('ko', { reload: false });
    quietServer();
    vi.spyOn(api, 'item').mockImplementation((id: string) =>
      Promise.resolve(item(id === 'A' ? 'A' : 'B')),
    );
    vi.spyOn(api, 'updateItem').mockResolvedValue(item('A'));
    const view = render(ItemScreen, { props: { workspace, itemId: 'A', workspaces: [workspace] } });
    await screen.findByRole('heading', { name: '항목 A' });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await screen.findByText('저장했습니다.');

    await view.rerender({ workspace, itemId: 'B', workspaces: [workspace] });
    await screen.findByRole('heading', { name: '항목 B' });
    expect(screen.queryByText('저장했습니다.')).toBeNull();
  });
});

function summary(id: string, title: string): SummaryView {
  return {
    id,
    title,
    parent_id: null,
    reference: null,
    revision: 1,
    updated_at: '2026-10-19T00:00:00Z',
  };
}

interface Rerender {
  rerender: (props: {
    workspace: Workspace;
    itemId: string;
    workspaces: Workspace[];
  }) => Promise<void>;
}

/** B로 옮겨 갔다가 A로 돌아온다. 돌아온 A는 새로 읽은 화면이다(제목이 `title`일 때까지 기다린다). */
async function leaveAndComeBack(view: Rerender, title: string): Promise<void> {
  await view.rerender({ workspace, itemId: 'B', workspaces: [workspace] });
  await screen.findByRole('heading', { name: '항목 B' });
  await view.rerender({ workspace, itemId: 'A', workspaces: [workspace] });
  await screen.findByRole('heading', { name: title });
}

// A → B → A로 돌아오면 주소의 항목은 같아도 새로 읽은 화면이다. 항목 id만 맞춰 보면 앞 A 화면에서 보낸 저장·다시
// 읽기의 늦은 응답이 새 A 화면에 들어왔다(이 화면에서 하지 않은 저장을 "저장했습니다."·오류로 알리거나, 새로 읽은
// 값을 그보다 먼저 읽은 옛 값으로 되돌렸다). 요청을 시작한 화면이 아직 그 화면일 때만 결과를 넣는다.
describe('item screen results after leaving and coming back to the same item', () => {
  it('drops a late save sent before leaving', async () => {
    await setLocale('ko', { reload: false });
    quietServer();
    vi.spyOn(api, 'item').mockImplementation((id: string) =>
      Promise.resolve(item(id === 'A' ? 'A' : 'B')),
    );
    const write = deferred<ItemView>();
    vi.spyOn(api, 'updateItem').mockReturnValue(write.promise);
    const view = render(ItemScreen, { props: { workspace, itemId: 'A', workspaces: [workspace] } });
    await screen.findByRole('heading', { name: '항목 A' });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));

    await leaveAndComeBack(view, '항목 A');
    write.resolve(item('A'));
    await settle();

    expect(screen.queryByText('저장했습니다.')).toBeNull();
  });

  it('drops a late save failure from before leaving', async () => {
    await setLocale('ko', { reload: false });
    quietServer();
    vi.spyOn(api, 'item').mockImplementation((id: string) =>
      Promise.resolve(item(id === 'A' ? 'A' : 'B')),
    );
    let fail: (reason: unknown) => void = () => undefined;
    vi.spyOn(api, 'updateItem').mockReturnValue(
      new Promise<ItemView>((_, reject) => {
        fail = reject;
      }),
    );
    const view = render(ItemScreen, { props: { workspace, itemId: 'A', workspaces: [workspace] } });
    await screen.findByRole('heading', { name: '항목 A' });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));

    await leaveAndComeBack(view, '항목 A');
    fail(new Error('offline'));
    await settle();

    expect(screen.queryByText('저장하지 못했습니다.')).toBeNull();
  });

  it('drops a late re-read started before leaving', async () => {
    await setLocale('ko', { reload: false });
    quietServer();
    const reread = deferred<ItemView>();
    let readsOfA = 0;
    vi.spyOn(api, 'item').mockImplementation((id: string) => {
      if (id !== 'A') return Promise.resolve(item('B'));
      readsOfA += 1;
      if (readsOfA === 2) return reread.promise;
      // 돌아와서 읽을 때는 그사이 다른 사람이 제목을 바꿨다.
      return Promise.resolve(readsOfA === 1 ? item('A') : { ...item('A'), title: '새 제목 A' });
    });
    vi.spyOn(api, 'itemProfile').mockResolvedValue(profile);
    vi.spyOn(api, 'setItemProfile').mockResolvedValue(undefined);
    const view = render(ItemScreen, { props: { workspace, itemId: 'A', workspaces: [workspace] } });
    const panel = await screen.findByRole('region', { name: '정보' });
    await fireEvent.click(await within(panel).findByRole('button', { name: '고치기' }));
    await fireEvent.click(within(panel).getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(readsOfA).toBe(2);
    });

    await leaveAndComeBack(view, '새 제목 A');
    // 떠나기 전에 읽은 옛 값.
    reread.resolve(item('A'));
    await settle();

    expect(heading()).toHaveTextContent('새 제목 A');
    expect(screen.getByRole('textbox', { name: '제목' })).toHaveValue('새 제목 A');
  });

  it('drops a late document list reload started before leaving', async () => {
    await setLocale('ko', { reload: false });
    vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
    vi.spyOn(api, 'module').mockResolvedValue(mod);
    vi.spyOn(api, 'item').mockImplementation((id: string) =>
      Promise.resolve(item(id === 'A' ? 'A' : 'B')),
    );
    const reload = deferred<SummaryView[]>();
    const two = [summary('d1', '첫 문서'), summary('d2', '둘째 문서')];
    let listsOfA = 0;
    vi.spyOn(api, 'itemDocuments').mockImplementation((id: string) => {
      if (id !== 'A') return Promise.resolve([]);
      listsOfA += 1;
      if (listsOfA === 2) return reload.promise;
      // 돌아와서 읽을 때는 그사이 다른 사람이 문서를 하나 더 만들었다.
      return Promise.resolve(listsOfA === 1 ? two : [...two, summary('d3', '셋째 문서')]);
    });
    // 옮기기가 실패해도 목록은 다시 읽는다. 이 테스트는 그 다시 읽기만 본다.
    vi.spyOn(api, 'setParent').mockRejectedValue(new Error('conflict'));
    const view = render(ItemScreen, { props: { workspace, itemId: 'A', workspaces: [workspace] } });
    const rows = await screen.findAllByRole('treeitem');
    const second = rows[1];
    if (second === undefined) throw new Error('no second tree item');
    await fireEvent.keyDown(second, { key: 'ArrowUp', altKey: true });
    await waitFor(() => {
      expect(listsOfA).toBe(2);
    });

    await leaveAndComeBack(view, '항목 A');
    await screen.findByText('셋째 문서');
    // 떠나기 전에 읽은 옛 목록.
    reload.resolve(two);
    await settle();

    expect(screen.getByText('셋째 문서')).toBeInTheDocument();
  });
});
