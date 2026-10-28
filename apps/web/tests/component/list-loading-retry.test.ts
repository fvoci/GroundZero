import { api } from '$lib/api/client';
import { eventsApi } from '$lib/features/events/api';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView } from '$lib/api/client';

import AuditScreen from '$lib/features/audit/AuditScreen.svelte';
import ItemEvents from '$lib/features/events/ItemEvents.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 목록 화면: 받기 전에는 "없습니다" 대신 불러오는 중, 첫 읽기가 실패하면 "없습니다"를 보이지 않고 다시 시도를 둔다.
beforeEach(async () => {
  await setLocale('ko', { reload: false });
});
afterEach(() => {
  vi.restoreAllMocks();
});

const item: ItemView = {
  id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  reference: 'PRJ-1',
  title: '항목',
  description: '',
  visibility: 'workspace',
  access: 'write',
  archived: false,
  updated_at: '2026-10-19T00:00:00Z',
  status: 'active',
  is_template: false,
  version: 1,
  starts_on: null,
  ends_on: null,
};

describe('audit list', () => {
  it('says loading, not empty, before the first page arrives', () => {
    vi.spyOn(api, 'audit').mockReturnValue(new Promise(() => undefined));
    render(AuditScreen, { props: { workspace: undefined } });
    expect(screen.queryByText('기록이 없습니다.')).toBeNull();
    expect(screen.getAllByText('불러오는 중…').length).toBeGreaterThan(0);
  });

  it('retries a failed first load instead of claiming there are no entries', async () => {
    const load = vi
      .spyOn(api, 'audit')
      .mockRejectedValueOnce(new Error('offline'))
      .mockResolvedValue([]);
    render(AuditScreen, { props: { workspace: undefined } });

    const retry = await screen.findByRole('button', { name: '다시 시도' });
    expect(screen.queryByText('기록이 없습니다.')).toBeNull();
    await fireEvent.click(retry);
    await waitFor(() => {
      expect(load).toHaveBeenCalledTimes(2);
    });
    expect(await screen.findByText('기록이 없습니다.')).toBeInTheDocument();
  });
});

describe('item events list', () => {
  it('says loading before the list, and retries a failed first load', async () => {
    const load = vi
      .spyOn(eventsApi, 'itemEvents')
      .mockRejectedValueOnce(new Error('offline'))
      .mockResolvedValue([]);
    render(ItemEvents, { props: { item, onchanged: () => undefined } });

    expect(screen.queryByText('일정이 없습니다.')).toBeNull();
    expect(screen.getByText('불러오는 중…')).toBeInTheDocument();
    const retry = await screen.findByRole('button', { name: '다시 시도' });
    expect(screen.queryByText('일정이 없습니다.')).toBeNull();
    await fireEvent.click(retry);
    await waitFor(() => {
      expect(load).toHaveBeenCalledTimes(2);
    });
    expect(await screen.findByText('일정이 없습니다.')).toBeInTheDocument();
  });
});
