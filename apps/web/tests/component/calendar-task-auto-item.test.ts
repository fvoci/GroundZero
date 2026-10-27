import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import CalendarTaskDialog from '$lib/features/events/CalendarTaskDialog.svelte';
import { m } from '$lib/paraglide/messages.js';

// F-2(R5 B): 모듈 범위에서 항목 목록을 읽으면 첫 항목을 자동으로 고른다. 그 선택을 사용자의 수정으로 세어, 아무것도
// 입력하지 않고 닫아도 초안을 남기고 "변경 버리고 닫기"를 물었다(D-97: 수정이 없으면 바로 닫는다).
afterEach(() => {
  vi.restoreAllMocks();
});

function itemView(id: string, title: string): ItemView {
  return {
    id,
    module_id: 'm1',
    workspace_id: 'ws-1',
    reference: `KED-${id}`,
    title,
    description: '',
    visibility: 'workspace',
    access: 'write',
    archived: false,
    updated_at: '2026-10-21T00:00:00Z',
    status: 'active',
    is_template: false,
    version: 1,
    starts_on: null,
    ends_on: null,
  };
}

function draftKeys(): string[] {
  return Object.keys(localStorage).filter((key) => key.includes(':form-draft:'));
}

describe('calendar task dialog in a module scope', () => {
  it('closes at once without a draft when only the item was picked automatically', async () => {
    vi.spyOn(api, 'items').mockResolvedValue([itemView('i1', '첫 항목'), itemView('i2', '둘째')]);
    const onclose = vi.fn();
    render(CalendarTaskDialog, {
      props: {
        workspaceId: 'ws-1',
        day: '2026-10-20',
        moduleId: 'm1',
        onclose,
        oncreated: vi.fn(),
      },
    });
    await waitFor(() => {
      expect(screen.getByRole('option', { name: '첫 항목' })).toHaveProperty('selected', true);
    });
    expect(draftKeys()).toEqual([]);
    await fireEvent.click(screen.getByRole('button', { name: m.common_cancel() }));
    expect(onclose).toHaveBeenCalledOnce();
    expect(screen.queryByRole('button', { name: m.common_discard_confirm() })).toBeNull();
    expect(draftKeys()).toEqual([]);
  });

  it('still protects an item the user picked over the automatic one', async () => {
    vi.spyOn(api, 'items').mockResolvedValue([itemView('i1', '첫 항목'), itemView('i2', '둘째')]);
    const onclose = vi.fn();
    render(CalendarTaskDialog, {
      props: {
        workspaceId: 'ws-1',
        day: '2026-10-20',
        moduleId: 'm1',
        onclose,
        oncreated: vi.fn(),
      },
    });
    const select = await screen.findByRole('combobox');
    await screen.findByRole('option', { name: '둘째' });
    await fireEvent.change(select, { target: { value: 'i2' } });
    await fireEvent.click(screen.getByRole('button', { name: m.common_cancel() }));
    expect(
      await screen.findByRole('button', { name: m.common_discard_confirm() }),
    ).toBeInTheDocument();
    expect(onclose).not.toHaveBeenCalled();
    expect(draftKeys()).toHaveLength(1);
  });
});
import './fixtures/event-user';
