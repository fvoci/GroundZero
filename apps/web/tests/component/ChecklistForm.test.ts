import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import ChecklistForm from '$lib/features/checklists/ChecklistForm.svelte';
import type { ChecklistItem } from '$lib/features/checklists/checklist-item';

describe('ChecklistForm', () => {
  it('adds an item from the input and reports the change', async () => {
    const onchange = vi.fn<(items: ChecklistItem[]) => void>();
    render(ChecklistForm, { items: [], onchange });
    const input = screen.getByLabelText('새 항목');
    await fireEvent.input(input, { target: { value: '자료 수집' } });
    await fireEvent.click(screen.getByRole('button', { name: '추가' }));
    expect(onchange).toHaveBeenCalledTimes(1);
    const [items] = onchange.mock.calls[0] ?? [];
    expect(items?.map((i) => i.text)).toEqual(['자료 수집']);
    expect(screen.getByLabelText('자료 수집')).toBeInTheDocument();
  });

  it('toggles completion through the checkbox', async () => {
    const onchange = vi.fn<(items: ChecklistItem[]) => void>();
    render(ChecklistForm, {
      items: [{ id: 'a', text: '초안 작성', done: false }],
      onchange,
    });
    await fireEvent.click(screen.getByRole('checkbox', { name: '초안 작성' }));
    const [items] = onchange.mock.calls[0] ?? [];
    expect(items?.[0]?.done).toBe(true);
  });

  it('does not submit an empty item', () => {
    const onchange = vi.fn<(items: ChecklistItem[]) => void>();
    render(ChecklistForm, { items: [], onchange });
    expect(screen.getByRole('button', { name: '추가' })).toBeDisabled();
    expect(onchange).not.toHaveBeenCalled();
  });
});
