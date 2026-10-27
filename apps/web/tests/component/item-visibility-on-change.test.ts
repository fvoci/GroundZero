import type { Module, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 감사 W13: 항목 정보 저장이 연 때의 공개 범위를 늘 보내, 그사이 다른 소유 권한자가 바꾼 공개 범위를 설명만 고친
// 저장이 되돌렸다. 공개 범위는 체크를 바꿨을 때만 보낸다(서버는 없으면 지금 값 그대로, 0084).
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const workspace: Workspace = {
  id: 'ws-1',
  organization_id: 'org-1',
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

const loaded: ItemView = {
  id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  reference: 'DEV-1',
  title: '항목',
  description: '원래 설명',
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

function setup(): void {
  // 하위 칸(업무·일정·기록 등)의 요청은 끝나지 않게 둔다.
  vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
  vi.spyOn(api, 'item').mockResolvedValue(loaded);
  vi.spyOn(api, 'module').mockResolvedValue(mod);
  vi.spyOn(api, 'itemDocuments').mockResolvedValue([]);
}

async function submitInfo(): Promise<void> {
  const title = await waitFor(() => {
    const el = document.getElementById('item-title');
    if (!(el instanceof HTMLInputElement)) throw new Error('item-title');
    return el;
  });
  const form = title.closest('form');
  if (form === null) throw new Error('form');
  await fireEvent.submit(form);
}

describe('item info save and visibility', () => {
  it('does not send the visibility when only the description changed', async () => {
    await setLocale('ko', { reload: false });
    setup();
    const update = vi.spyOn(api, 'updateItem').mockResolvedValue({ kind: 'updated', item: loaded });
    render(ItemScreen, { props: { workspace, itemId: 'i1', workspaces: [workspace] } });

    const description = await waitFor(() => {
      const el = document.getElementById('item-description');
      if (!(el instanceof HTMLTextAreaElement)) throw new Error('item-description');
      return el;
    });
    await fireEvent.input(description, { target: { value: '오타 고침' } });
    await submitInfo();
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(1);
    });
    expect(update.mock.calls[0]?.[2]).toEqual({ title: '항목', description: '오타 고침' });
  });

  it('sends the visibility the owner chose', async () => {
    await setLocale('ko', { reload: false });
    setup();
    const update = vi.spyOn(api, 'updateItem').mockResolvedValue({ kind: 'updated', item: loaded });
    render(ItemScreen, { props: { workspace, itemId: 'i1', workspaces: [workspace] } });

    await fireEvent.click(await screen.findByLabelText(/워크스페이스 멤버에게 공개/));
    await submitInfo();
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(1);
    });
    expect(update.mock.calls[0]?.[2]).toEqual({
      title: '항목',
      description: '원래 설명',
      visibility: 'private',
    });
  });
});
