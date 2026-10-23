import { render, screen, waitFor } from '@testing-library/svelte';
import { SvelteMap } from 'svelte/reactivity';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { EventView, ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import ItemEvents from '$lib/features/events/ItemEvents.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

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

function event(id: string, title: string): EventView {
  return {
    id,
    item_id: 'i1',
    workspace_id: 'ws-1',
    kind: 'meeting',
    title,
    description: '',
    all_day: true,
    start_date: '2026-10-21',
    end_date: '2026-10-21',
    starts_at: null,
    ends_at: null,
    timezone: 'Asia/Seoul',
    recurrence: null,
    excluded_dates: [],
    version: 1,
    access: 'write',
  };
}

// 같은 화면의 달력이 일정을 바꿀 때마다(version) 목록을 다시 읽는다. 먼저 보낸 읽기가 늦게 오면 그 사이 끝난 새
// 읽기의 목록을 덮어 목록과 달력이 다시 어긋났다. 마지막으로 보낸 읽기의 결과만 남는다.
describe('item events list', () => {
  it('keeps the newest read when an older read answers later', async () => {
    await setLocale('ko', { reload: false });
    const answers: ((list: EventView[]) => void)[] = [];
    vi.spyOn(api, 'itemEvents').mockImplementation(
      () =>
        new Promise((resolve) => {
          answers.push(resolve);
        }),
    );
    const shared = new SvelteMap([['version', 0]]);
    render(ItemEvents, {
      props: {
        item,
        get version(): number {
          return shared.get('version') ?? 0;
        },
      },
    });
    await waitFor(() => {
      expect(answers).toHaveLength(1);
    });
    answers[0]?.([]);
    shared.set('version', 1);
    await waitFor(() => {
      expect(answers).toHaveLength(2);
    });
    shared.set('version', 2);
    await waitFor(() => {
      expect(answers).toHaveLength(3);
    });
    // 새 읽기(일정 추가 뒤)가 먼저, 옛 읽기(추가 전)가 나중에 온다.
    answers[2]?.([event('e1', '중간 발표')]);
    expect(await screen.findByText('중간 발표')).toBeInTheDocument();
    answers[1]?.([]);
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.getByText('중간 발표')).toBeInTheDocument();
  });
});
