import './fixtures/task-statuses';
import type { Module, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ItemView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
import WorkspaceScreen from '$lib/features/workspaces/WorkspaceScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

function workspace(id: string): Workspace {
  return {
    access: 'write',
    id,
    name: `공간 ${id}`,
    organization_id: 'org-1',
    role: 'member',
    is_owner: false,
    realtime_enabled: false,
  };
}

const templateOfA = { id: 't-a', title: 'A 회의록 양식', reference: 'WIKI-1', personal: false };

/** 워크스페이스 A에만 문서 템플릿이 있다. 그 밖의 요청(문서 목록·모듈·하위 칸)은 끝나지 않게 둔다. */
function mockTemplates(): void {
  vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
  vi.spyOn(api, 'documentTemplates').mockImplementation((ws: string | null) =>
    Promise.resolve(ws === 'A' ? [templateOfA] : []),
  );
}

function input(id: string): HTMLInputElement | HTMLSelectElement {
  const el = document.getElementById(id);
  if (el instanceof HTMLInputElement || el instanceof HTMLSelectElement) return el;
  throw new Error(id);
}

function item(id: string, ws: string): ItemView {
  return {
    id,
    module_id: `m-${ws}`,
    workspace_id: ws,
    reference: `DEV-${id}`,
    title: `항목 ${id}`,
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

function module(ws: string): Module {
  return {
    access: 'owner',
    id: `m-${ws}`,
    workspace_id: ws,
    kind: 'project',
    name: '개발',
    key: 'DEV',
    previous_keys: [],
    archived: false,
    icon: '',
    description: '',
  };
}

// 감사 W28: 워크스페이스(또는 항목) 화면은 옮겨 가도 다시 만들어지지 않는다. A에서 고른 문서 템플릿이 남아, B의
// 목록에는 없어 선택 칸이 사라졌는데도 B의 새 문서가 A의 템플릿 내용으로 만들어졌다.
describe('document template after switching', () => {
  it('does not use the template picked in the previous workspace', async () => {
    await setLocale('ko', { reload: false });
    mockTemplates();
    const create = vi
      .spyOn(api, 'createWikiDocument')
      .mockRejectedValue(new ApiError(400, 'bad_request', 'stop here'));
    const view = render(WorkspaceScreen, { props: { workspace: workspace('A') } });
    await screen.findByRole('option', { name: /A 회의록 양식/ });
    await fireEvent.change(input('new-document-template'), { target: { value: 't-a' } });

    await view.rerender({ workspace: workspace('B') });
    await waitFor(() => {
      expect(screen.queryByRole('option', { name: /A 회의록 양식/ })).toBeNull();
    });
    await fireEvent.input(input('new-document'), { target: { value: '회의록' } });
    await fireEvent.click(screen.getByRole('button', { name: '문서 만들기' }));

    await waitFor(() => {
      expect(create).toHaveBeenCalledTimes(1);
    });
    expect(create.mock.calls[0]?.[0]).toBe('B');
    expect(create.mock.calls[0]?.[4]).toBeNull();
  });

  it('does not use the template picked on an item of another workspace', async () => {
    await setLocale('ko', { reload: false });
    mockTemplates();
    vi.spyOn(api, 'item').mockImplementation((id: string) =>
      Promise.resolve(id === 'ia' ? item('ia', 'A') : item('ib', 'B')),
    );
    vi.spyOn(api, 'module').mockImplementation((id: string) =>
      Promise.resolve(module(id === 'm-A' ? 'A' : 'B')),
    );
    vi.spyOn(api, 'itemDocuments').mockResolvedValue([]);
    const create = vi
      .spyOn(api, 'createItemDocument')
      .mockRejectedValue(new ApiError(400, 'bad_request', 'stop here'));
    const workspaces = [workspace('A'), workspace('B')];
    const view = render(ItemScreen, {
      props: { workspace: workspace('A'), itemId: 'ia', workspaces },
    });
    await screen.findByRole('option', { name: /A 회의록 양식/ });
    await fireEvent.change(input('new-item-document-template'), { target: { value: 't-a' } });

    // 참조·역링크를 따라 다른 워크스페이스의 항목으로 간다.
    await view.rerender({ workspace: workspace('B'), itemId: 'ib', workspaces });
    await screen.findByRole('heading', { name: '항목 ib' });
    await waitFor(() => {
      expect(screen.queryByRole('option', { name: /A 회의록 양식/ })).toBeNull();
    });
    await fireEvent.input(input('new-item-document'), { target: { value: '회의록' } });
    await fireEvent.click(screen.getByRole('button', { name: '문서 만들기' }));

    await waitFor(() => {
      expect(create).toHaveBeenCalledTimes(1);
    });
    expect(create.mock.calls[0]?.[0]).toBe('ib');
    expect(create.mock.calls[0]?.[4]).toBeNull();
  });

  // 리뷰: 주소의 워크스페이스만 바뀌고 항목은 그대로면(앱 안의 링크로는 생기지 않고 주소를 손으로 고친 경우) 항목
  // 화면은 고른 템플릿을 비우지 않는다. 새 워크스페이스의 템플릿 목록을 읽지 못하면 선택 칸이 사라진 채로 앞에서
  // 고른 템플릿이 새 문서에 쓰였다.
  it('drops the picked template when only the workspace changes and its list fails to load', async () => {
    await setLocale('ko', { reload: false });
    vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
    const templates = vi
      .spyOn(api, 'documentTemplates')
      .mockImplementation((ws: string | null) =>
        ws === 'A'
          ? Promise.resolve([templateOfA])
          : Promise.reject(new ApiError(500, 'internal', 'down')),
      );
    vi.spyOn(api, 'item').mockResolvedValue(item('ia', 'A'));
    vi.spyOn(api, 'module').mockResolvedValue(module('A'));
    vi.spyOn(api, 'itemDocuments').mockResolvedValue([]);
    const create = vi
      .spyOn(api, 'createItemDocument')
      .mockRejectedValue(new ApiError(400, 'bad_request', 'stop here'));
    const workspaces = [workspace('A'), workspace('B')];
    const view = render(ItemScreen, {
      props: { workspace: workspace('A'), itemId: 'ia', workspaces },
    });
    await screen.findByRole('option', { name: /A 회의록 양식/ });
    await fireEvent.change(input('new-item-document-template'), { target: { value: 't-a' } });
    // 고르는 것만으로는 목록을 다시 읽거나 값을 비우지 않는다.
    expect(input('new-item-document-template').value).toBe('t-a');
    expect(templates).toHaveBeenCalledTimes(1);

    await view.rerender({ workspace: workspace('B'), itemId: 'ia', workspaces });
    await waitFor(() => {
      expect(templates).toHaveBeenLastCalledWith('B');
    });
    expect(screen.queryByRole('option', { name: /A 회의록 양식/ })).toBeNull();
    await fireEvent.input(input('new-item-document'), { target: { value: '회의록' } });
    await fireEvent.click(screen.getByRole('button', { name: '문서 만들기' }));

    await waitFor(() => {
      expect(create).toHaveBeenCalledTimes(1);
    });
    expect(create.mock.calls[0]?.[0]).toBe('ia');
    expect(create.mock.calls[0]?.[4]).toBeNull();
  });
});
