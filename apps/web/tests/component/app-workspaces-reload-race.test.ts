import { timeData } from '$lib/features/time/data';
import type { Me, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import * as feedback from '$lib/features/feedback/feedback.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

import { TEST_ORGANIZATION } from './fixtures/organization';

import App from '../../src/app/App.svelte';

const WS = '11111111-1111-4111-8111-111111111111';
const RESTORED = '22222222-2222-4222-8222-222222222222';
const LABEL = '실시간 공동 편집(문서를 여럿이 동시에 고침)';

const me: Me = {
  id: 'user-a',
  display_name: '나',
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

function workspace(id: string, name: string, realtime: boolean): Workspace {
  return {
    access: 'owner',
    id,
    organization_id: 'org-1',
    name,
    role: 'admin',
    is_owner: true,
    realtime_enabled: realtime,
  };
}

function checkbox(): HTMLInputElement {
  const box = screen.getByRole('checkbox', { name: LABEL });
  if (!(box instanceof HTMLInputElement)) throw new Error('checkbox');
  return box;
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  // jsdom에는 matchMedia가 없다(사이드바가 좁은 화면 판정에 쓴다).
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
  // 따로 흉내 내지 않은 요청(화면 곳곳이 스스로 불러오는 것)은 없다고 답한다.
  vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
    Promise.resolve(
      new Response(JSON.stringify({ error: 'not_found', message: 'not found' }), {
        status: 404,
        headers: { 'Content-Type': 'application/json' },
      }),
    ),
  );
  vi.spyOn(api, 'me').mockResolvedValue(me);
  vi.spyOn(api, 'myOrganizations').mockResolvedValue([TEST_ORGANIZATION]);
  vi.spyOn(api, 'unreadCount').mockResolvedValue(0);
  vi.spyOn(api, 'notifications').mockResolvedValue({ notifications: [], next: null });
  vi.spyOn(timeData, 'currentTimer').mockResolvedValue(null);
  vi.spyOn(api, 'wikiDocuments').mockResolvedValue([]);
  vi.spyOn(api, 'modules').mockResolvedValue([]);
  vi.spyOn(api, 'documentTemplates').mockResolvedValue([]);
  vi.spyOn(api, 'deletedWorkspaces').mockResolvedValue([
    {
      id: RESTORED,
      name: '지운 공간',
      deleted_at: '2026-10-19T00:00:00Z',
      expires_at: '2026-11-19T00:00:00Z',
      expired: false,
    },
  ]);
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  router.guard = undefined;
  router.navigate('/');
});

// 워크스페이스 목록을 다시 읽는 중(되살리기 뒤 등)에 실시간 공동 편집을 바꾸면, 그 전에 보낸 목록 요청의 응답(바꾸기 전
// 값)이 늦게 와서 방금 저장한 값을 덮으면 안 된다.
describe('workspace list reload and the realtime setting', () => {
  it('keeps the realtime value just saved when an older list arrives later', async () => {
    // 서버: 실시간 설정은 저장 요청을 받으면 바뀌고, 목록 응답은 요청을 받은 때의 값을 담는다.
    let serverRealtime = false;
    let restored = false;
    vi.spyOn(api, 'restoreWorkspace').mockImplementation(() => {
      restored = true;
      return Promise.resolve(undefined);
    });
    vi.spyOn(api, 'setWorkspaceRealtime').mockImplementation((_id, enabled) => {
      serverRealtime = enabled;
      return Promise.resolve(undefined);
    });
    let calls = 0;
    const late: (() => void)[] = [];
    vi.spyOn(api, 'workspaces').mockImplementation(() => {
      calls += 1;
      const list = [
        workspace(WS, '공간', serverRealtime),
        ...(restored ? [workspace(RESTORED, '지운 공간', false)] : []),
      ];
      // 되살린 뒤 다시 읽는 목록(두 번째 요청)의 응답은 늦게 온다.
      if (calls !== 2) return Promise.resolve(list);
      return new Promise<Workspace[]>((resolve) => {
        late.push(() => {
          resolve(list);
        });
      });
    });

    router.navigate('/trash');
    render(App);
    await fireEvent.click(await screen.findByRole('button', { name: '지운 공간 되살리기' }));
    await waitFor(() => {
      expect(calls).toBe(2);
    });

    // 목록 응답을 기다리는 사이 워크스페이스 화면에서 실시간 공동 편집을 켠다.
    router.navigate(`/w/${WS}`);
    await screen.findByRole('checkbox', { name: LABEL });
    expect(checkbox().checked).toBe(false);
    const done = vi.spyOn(feedback, 'notifyDone');
    await fireEvent.click(checkbox());
    // 저장이 끝나면 완료 토스트를 띄운다(D-82).
    await waitFor(() => {
      expect(done).toHaveBeenCalledWith(
        '실시간 공동 편집을 켰습니다. 문서를 다시 열면 함께 고칠 수 있습니다.',
        'workspace-realtime',
      );
    });
    expect(serverRealtime).toBe(true);

    // 켜기 전에 보낸 목록 요청의 응답이 이제 도착한다.
    for (const resolve of late) resolve();
    await tick();
    await new Promise((r) => setTimeout(r, 0));
    await tick();

    expect(checkbox().checked).toBe(true);
    // 되살린 워크스페이스는 목록에 들어와 있다(늦은 응답을 버려도 그 변화는 잃지 않는다).
    router.navigate(`/w/${RESTORED}`);
    expect(await screen.findByText('지운 공간 · 위키')).toBeInTheDocument();
  });
});
