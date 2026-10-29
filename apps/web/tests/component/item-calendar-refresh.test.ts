import type { Workspace } from '@fvoci/api-client';
import { render, waitFor } from '@testing-library/svelte';
import { SvelteMap } from 'svelte/reactivity';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { CalendarView } from '$lib/api/client';
import { api } from '$lib/api/client';
import CalendarScreen from '$lib/features/events/CalendarScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
});

const workspace: Workspace = {
  access: 'owner',
  id: 'ws-1',
  organization_id: 'org-1',
  name: '공간',
  role: 'admin',
  is_owner: true,
};

const empty: CalendarView = {
  tasks: [],
  projected_tasks: [],
  events: [],
  occurrences: [],
  dates: [],
  items: [],
};

// 항목 화면: 일정 목록에서 일정을 만들면(version이 늘면) 같은 화면의 달력이 다시 읽는다. 새로고침해야 보이면 안 된다.
// rerender는 props 전체를 바꿔 어떤 값이든 다시 읽게 하므로, version만 바뀌는 props로 확인한다.
describe('item calendar', () => {
  it('reads again when only the item events version changes', async () => {
    await setLocale('ko', { reload: false });
    const calendar = vi.spyOn(api, 'calendar').mockResolvedValue(empty);
    const shared = new SvelteMap([['version', 0]]);
    const props = {
      workspace,
      scope: { itemId: 'i1' },
      embedded: true,
      get version(): number {
        return shared.get('version') ?? 0;
      },
    };
    render(CalendarScreen, { props });
    await waitFor(() => {
      expect(calendar).toHaveBeenCalledTimes(1);
    });
    expect(calendar.mock.calls[0]?.[4]).toEqual({ itemId: 'i1' });

    shared.set('version', 1);
    await waitFor(() => {
      expect(calendar).toHaveBeenCalledTimes(2);
    });
  });
});
