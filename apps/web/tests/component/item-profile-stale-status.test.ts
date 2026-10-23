import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemProfileView, ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import ItemProfilePanel from '$lib/features/modules/ItemProfilePanel.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 앞서 저장에 성공한 안내("항목 정보를 저장했습니다.")가 다음 저장의 409 알림 옆에 남으면, 입력이 저장된 것처럼 보여 보존한
// 입력을 두고 떠나게 한다(실제 브라우저 확인 w-browser2 B에서 드러남). 저장을 보낼 때 앞선 안내를 지운다.

const item: ItemView = {
  id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  reference: 'DEV-1',
  title: '항목',
  description: '',
  visibility: 'workspace',
  access: 'owner',
  archived: false,
  updated_at: '2026-10-23T00:00:00Z',
  status: 'active',
  is_template: false,
  version: 1,
  starts_on: null,
  ends_on: null,
};

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
  version: 1,
};

function response(status: number, json: unknown): Response {
  return new Response(json === undefined ? null : JSON.stringify(json), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

/** `PUT /api/items/i1/profile`에 차례로 `replies`를 돌려준다. */
function serve(replies: [number, unknown][]): void {
  vi.stubGlobal('fetch', (input: unknown, init?: RequestInit) => {
    if (input === '/api/items/i1/profile' && init?.method === 'PUT') {
      const [status, json] = replies.shift() ?? [500, { error: 'internal', message: 'x' }];
      return Promise.resolve(response(status, json));
    }
    return new Promise<Response>(() => undefined);
  });
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  Element.prototype.hasPointerCapture = (): boolean => false;
  Element.prototype.setPointerCapture = (): void => undefined;
  Element.prototype.releasePointerCapture = (): void => undefined;
  Element.prototype.scrollIntoView = (): void => undefined;
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('item profile status line', () => {
  it('drops the earlier "saved" line when the next save conflicts', async () => {
    vi.spyOn(api, 'itemProfile').mockResolvedValue(profile);
    vi.spyOn(api, 'assignable').mockResolvedValue([]);
    serve([
      [204, undefined],
      [409, { error: 'conflict', current: { ...profile, goal: '다른 탭', version: 2 } }],
    ]);
    render(ItemProfilePanel, { props: { item, kind: 'project', onchanged: () => undefined } });

    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    expect(await screen.findByText('항목 정보를 저장했습니다.')).toBeInTheDocument();

    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await screen.findByText(/그사이 다른 사람이 항목 정보를 바꿔/);
    await waitFor(() => {
      expect(screen.queryByText('항목 정보를 저장했습니다.')).not.toBeInTheDocument();
    });
  });
});
