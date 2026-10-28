import { workApi as work } from '$lib/features/tasks/api';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemProfileView, ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import ItemProfilePanel from '$lib/features/modules/ItemProfilePanel.svelte';
import { m } from '$lib/paraglide/messages.js';
import { notifyDone } from '$lib/features/feedback/feedback.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { me } from '$lib/me.svelte';
import './fixtures/authenticated';

vi.mock('$lib/features/feedback/feedback.svelte', async (importOriginal) => ({
  ...(await importOriginal<typeof import('$lib/features/feedback/feedback.svelte')>()),
  notifyDone: vi.fn(),
}));

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
  vi.mocked(notifyDone).mockClear();
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
    vi.spyOn(work, 'assignable').mockResolvedValue([]);
    serve([
      [204, undefined],
      [409, { error: 'conflict', current: { ...profile, goal: '다른 탭', version: 2 } }],
    ]);
    render(ItemProfilePanel, { props: { item, kind: 'project', onchanged: () => undefined } });

    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(notifyDone).toHaveBeenCalledWith('항목 정보를 저장했습니다.');
    });

    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await screen.findByText(/그사이 다른 사람이 항목 정보를 바꿔/);
    await waitFor(() => {
      expect(screen.queryByText('항목 정보를 저장했습니다.')).not.toBeInTheDocument();
    });
  });

  it('closes the form only after the saved profile is reread', async () => {
    // 저장은 204라 새 판을 주지 않는다. 다시 읽기 전에 편집을 다시 열면 옛 판을 기준으로 삼아 다음 저장이 거짓 409가 된다.
    let reread: (p: ItemProfileView) => void = () => undefined;
    vi.spyOn(api, 'itemProfile')
      .mockResolvedValueOnce(profile)
      .mockImplementationOnce(
        () =>
          new Promise<ItemProfileView>((resolve) => {
            reread = resolve;
          }),
      )
      .mockResolvedValue({ ...profile, goal: '새 목표', version: 2 });
    vi.spyOn(work, 'assignable').mockResolvedValue([]);
    const sent: Record<string, unknown>[] = [];
    vi.stubGlobal('fetch', (input: unknown, init?: RequestInit) => {
      if (input === '/api/items/i1/profile' && init?.method === 'PUT') {
        const body: unknown = JSON.parse(typeof init.body === 'string' ? init.body : '{}');
        if (typeof body === 'object' && body !== null) sent.push({ ...body });
        return Promise.resolve(response(204, undefined));
      }
      return new Promise<Response>(() => undefined);
    });
    render(ItemProfilePanel, { props: { item, kind: 'project', onchanged: () => undefined } });

    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(screen.queryByRole('button', { name: '고치기' })).not.toBeInTheDocument();

    reread({ ...profile, goal: '새 목표', version: 2 });
    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(sent).toHaveLength(2);
    });
    expect(sent[1]).toMatchObject({ base_version: 2 });
  });

  it('does not close an edit opened again while the earlier save was being reread', async () => {
    let reread: (p: ItemProfileView) => void = () => undefined;
    vi.spyOn(api, 'itemProfile')
      .mockResolvedValueOnce(profile)
      .mockImplementationOnce(
        () =>
          new Promise<ItemProfileView>((resolve) => {
            reread = resolve;
          }),
      )
      .mockResolvedValue({ ...profile, version: 2 });
    vi.spyOn(work, 'assignable').mockResolvedValue([]);
    serve([[204, undefined]]);
    render(ItemProfilePanel, { props: { item, kind: 'project', onchanged: () => undefined } });

    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    const goal = document.querySelector<HTMLTextAreaElement>('#item-goal');
    if (goal === null) throw new Error('goal');
    await fireEvent.input(goal, { target: { value: '새로 쓰는 목표' } });

    reread({ ...profile, version: 2 });
    await waitFor(() => {
      expect(api.itemProfile).toHaveBeenCalledTimes(2);
    });
    await Promise.resolve();
    expect(screen.getByRole('button', { name: '저장' })).toBeInTheDocument();
    expect(document.querySelector<HTMLTextAreaElement>('#item-goal')?.value).toBe('새로 쓰는 목표');
  });

  it('does not offer an edit on the old version when the reread fails', async () => {
    let reload: (p: ItemProfileView) => void = () => undefined;
    vi.spyOn(api, 'itemProfile')
      .mockResolvedValueOnce(profile)
      .mockRejectedValueOnce(new Error('offline'))
      .mockImplementationOnce(
        () =>
          new Promise<ItemProfileView>((resolve) => {
            reload = resolve;
          }),
      );
    vi.spyOn(work, 'assignable').mockResolvedValue([]);
    const sent: Record<string, unknown>[] = [];
    vi.stubGlobal('fetch', (input: unknown, init?: RequestInit) => {
      if (input === '/api/items/i1/profile' && init?.method === 'PUT') {
        const body: unknown = JSON.parse(typeof init.body === 'string' ? init.body : '{}');
        if (typeof body === 'object' && body !== null) sent.push({ ...body });
        return Promise.resolve(response(204, undefined));
      }
      return new Promise<Response>(() => undefined);
    });
    render(ItemProfilePanel, { props: { item, kind: 'project', onchanged: () => undefined } });

    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(api.itemProfile).toHaveBeenCalledTimes(3);
    });
    expect(screen.queryByRole('button', { name: '고치기' })).not.toBeInTheDocument();

    reload({ ...profile, version: 2 });
    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(sent).toHaveLength(2);
    });
    expect(sent[1]).toMatchObject({ base_version: 2 });
  });

  it('drops the earlier "saved" line when the period is invalid', async () => {
    vi.spyOn(api, 'itemProfile').mockResolvedValue(profile);
    vi.spyOn(work, 'assignable').mockResolvedValue([]);
    serve([[204, undefined]]);
    render(ItemProfilePanel, { props: { item, kind: 'project', onchanged: () => undefined } });

    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(notifyDone).toHaveBeenCalledWith('항목 정보를 저장했습니다.');
    });

    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    const start = document.querySelector<HTMLInputElement>('#item-starts');
    const end = document.querySelector<HTMLInputElement>('#item-ends');
    if (start === null || end === null) throw new Error('period inputs');
    await fireEvent.input(start, { target: { value: '2026-10-20' } });
    await fireEvent.input(end, { target: { value: '2026-10-10' } });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await screen.findByText(m.modules_period_invalid());
    expect(screen.queryByText('항목 정보를 저장했습니다.')).not.toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    await fireEvent.click(screen.getByRole('button', { name: m.common_discard_confirm() }));
    expect(screen.queryByRole('alert')).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: '고치기' }));
    expect(screen.queryByRole('alert')).toBeNull();
  });

  // FC-8: 저장하는 동안 취소는 잠겨 있다(D-97). jsdom은 비활성 단추에도 click을 보내므로 "저장 중 취소 뒤 늦은 오류"는
  // 브라우저에서 닿지 않는 경로였다. 닿는 경로는 늦은 실패가 열린 폼에 남고, 풀린 뒤의 취소·새 편집이 그 오류를 지우는 것이다.
  it('keeps cancel locked while saving and clears the late error on cancel and a new edit', async () => {
    vi.spyOn(api, 'itemProfile').mockResolvedValue(profile);
    vi.spyOn(work, 'assignable').mockResolvedValue([]);
    let fail: (e: Error) => void = () => undefined;
    const save = vi.spyOn(api, 'setItemProfile').mockImplementation(
      () =>
        new Promise((_resolve, reject) => {
          fail = reject;
        }),
    );
    render(ItemProfilePanel, { props: { item, kind: 'project', onchanged: () => undefined } });
    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(save).toHaveBeenCalledOnce();
    });
    expect(screen.getByRole('button', { name: '취소' })).toBeDisabled();
    expect(screen.getByRole('button', { name: '저장' })).toBeDisabled();
    await act(() => {
      fail(new Error('late failure'));
    });
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '취소' })).not.toBeDisabled();
    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    // 고친 칸이 없으므로 묻지 않고 닫는다.
    expect(screen.queryByRole('button', { name: m.common_discard_confirm() })).toBeNull();
    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    expect(screen.queryByRole('alert')).toBeNull();
  });
});
it('restores all profile text and the original base only for the original account', async () => {
  vi.spyOn(api, 'itemProfile').mockResolvedValue(profile);
  vi.spyOn(work, 'assignable').mockResolvedValue([]);
  const save = vi.spyOn(api, 'setItemProfile').mockResolvedValue({ kind: 'saved' });
  const view = render(ItemProfilePanel, { item, kind: 'project', onchanged: () => undefined });
  await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
  const goal = document.querySelector<HTMLTextAreaElement>('#item-goal');
  if (goal === null) throw new Error('goal');
  await fireEvent.input(goal, { target: { value: '긴 목표\n다음 줄' } });
  view.unmount();
  me.userId = 'other-account';
  const other = render(ItemProfilePanel, { item, kind: 'project', onchanged: () => undefined });
  await screen.findByRole('button', { name: '고치기' });
  expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
  other.unmount();
  me.userId = 'user-a';
  vi.mocked(api.itemProfile).mockResolvedValue({ ...profile, version: 9 });
  render(ItemProfilePanel, { item, kind: 'project', onchanged: () => undefined });
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(document.querySelector('#item-goal')).toHaveValue('긴 목표\n다음 줄');
  expect(save).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  await waitFor(() => {
    expect(save).toHaveBeenCalledWith(
      item.id,
      1,
      expect.objectContaining({ goal: '긴 목표\n다음 줄' }),
    );
  });
});
