import './fixtures/task-statuses';
import type { Module, Workspace } from '@fvoci/api-client';
import { render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import './fixtures/authenticated';

// CodeRabbit(#141 e5e001a5, outside diff): 서버 등급(app_grade_item)은 보관을 반영하지 않아 보관된 항목의 소유자도 owner를
// 받는다. 화면이 등급만 보면 편집 폼과 "보관" 단추(실제로는 해제)가 열리고 {:else} 분기의 "보관 해제" 단추가 숨는다. 보관된
// 항목·보관된 모듈의 항목은 읽기 전용이다(서버도 쓰기를 55000으로 거부, app_item_archived).
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const workspace: Workspace = {
  access: 'owner',
  id: 'ws-1',
  organization_id: 'org-1',
  name: '공간',
  role: 'admin',
  is_owner: true,
};

function module(over: Partial<Module> = {}): Module {
  return {
    access: 'owner',
    id: 'm1',
    workspace_id: 'ws-1',
    kind: 'project',
    name: '개발',
    key: 'DEV',
    previous_keys: [],
    archived: false,
    icon: '',
    description: '',
    ...over,
  };
}

function item(over: Partial<ItemView> = {}): ItemView {
  return {
    id: 'i1',
    module_id: 'm1',
    workspace_id: 'ws-1',
    reference: 'DEV-1',
    title: '항목',
    description: '설명',
    visibility: 'workspace',
    access: 'owner',
    archived: false,
    updated_at: '2026-10-19T00:00:00Z',
    status: 'active',
    is_template: false,
    version: 1,
    starts_on: null,
    ends_on: null,
    ...over,
  };
}

async function mount(current: ItemView, mod: Module): Promise<void> {
  await setLocale('ko', { reload: false });
  // 하위 칸(업무·일정·기록 등)의 나머지 요청은 끝나지 않게 둔다.
  vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
  vi.spyOn(api, 'item').mockResolvedValue(current);
  vi.spyOn(api, 'module').mockResolvedValue(mod);
  vi.spyOn(api, 'itemDocuments').mockResolvedValue([]);
  render(ItemScreen, { props: { workspace, itemId: 'i1', workspaces: [workspace] } });
  await screen.findByRole('heading', { name: '항목' });
}

describe('archived item screen', () => {
  it('shows the owner the unarchive button instead of the edit form', async () => {
    await mount(item({ archived: true }), module());
    expect(await screen.findByRole('button', { name: '보관 해제' })).toBeInTheDocument();
    expect(document.getElementById('item-title')).toBeNull();
    expect(screen.queryByRole('button', { name: '보관' })).toBeNull();
    expect(screen.getByText('보관됨 · 읽기 전용')).toBeInTheDocument();
  });

  it('keeps an item of an archived module read-only without offering archive buttons', async () => {
    await mount(item(), module({ archived: true }));
    expect(await screen.findByText('보관됨 · 읽기 전용')).toBeInTheDocument();
    expect(document.getElementById('item-title')).toBeNull();
    expect(screen.queryByRole('button', { name: '보관' })).toBeNull();
    expect(screen.queryByRole('button', { name: '보관 해제' })).toBeNull();
  });

  it('still lets the owner edit an item that is not archived', async () => {
    await mount(item(), module());
    expect(await screen.findByRole('button', { name: '보관' })).toBeInTheDocument();
    expect(document.getElementById('item-title')).not.toBeNull();
  });
});
