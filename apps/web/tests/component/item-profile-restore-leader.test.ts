import './fixtures/authenticated';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemProfileView, ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import ItemProfilePanel from '$lib/features/modules/ItemProfilePanel.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

// F-9(R5 B): 프로필 초안을 되살리면 편집만 열고 고를 수 있는 사람을 읽지 않았다. 리더 목록이 "없음"뿐이라 리더를 바꿀 수
// 없었고, 서버 리더와 다른 리더를 고른 초안은 이름이 "알 수 없음"으로 보였다.

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

const first = { user_id: 'u1', display_name: 'First User' };
const second = { user_id: 'u2', display_name: 'Second User' };

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

function leaderTrigger(): HTMLElement {
  const el = document.querySelector<HTMLElement>('[aria-labelledby="item-leader-label"]');
  if (el === null) throw new Error('leader trigger');
  return el;
}

async function openLeaders(): Promise<void> {
  const trigger = leaderTrigger();
  await fireEvent.pointerDown(trigger, { button: 0, pointerType: 'mouse' });
  await fireEvent.click(trigger);
}

async function pickLeader(name: string): Promise<void> {
  await openLeaders();
  const option = await screen.findByRole('option', { name });
  await fireEvent.pointerUp(option, { button: 0, pointerType: 'mouse' });
  await fireEvent.click(option);
}

async function draftWithLeader(): Promise<void> {
  vi.spyOn(api, 'itemProfile').mockResolvedValue(profile);
  const view = render(ItemProfilePanel, { item, kind: 'project', onchanged: vi.fn() });
  await fireEvent.click(await screen.findByRole('button', { name: m.modules_edit() }));
  await pickLeader('Second User');
  expect(leaderTrigger().textContent.trim()).toBe('Second User');
  view.unmount();
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
});

describe('restoring an item profile draft', () => {
  it('loads the assignable people so the leader can be changed again', async () => {
    vi.spyOn(api, 'assignable').mockResolvedValue([first, second]);
    await draftWithLeader();
    render(ItemProfilePanel, { item, kind: 'project', onchanged: vi.fn() });
    await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
    await waitFor(() => {
      expect(api.assignable).toHaveBeenCalledTimes(2);
    });
    await pickLeader('First User');
    expect(leaderTrigger().textContent.trim()).toBe('First User');
  });

  it('keeps the drafted leader name even if the people cannot be read', async () => {
    const assignable = vi.spyOn(api, 'assignable').mockResolvedValueOnce([first, second]);
    await draftWithLeader();
    assignable.mockRejectedValue(new Error('offline'));
    render(ItemProfilePanel, { item, kind: 'project', onchanged: vi.fn() });
    await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
    expect(leaderTrigger().textContent.trim()).toBe('Second User');
  });
});
