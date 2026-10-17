import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import ChecklistForm from '$lib/features/checklists/ChecklistForm.svelte';
import type { NewChecklistEntry } from '$lib/features/checklists/checklist-item';
import { parseMarkdownChecklist } from '$lib/features/checklists/checklist-item';

function handlers() {
  return {
    onadd: vi.fn<(items: NewChecklistEntry[]) => Promise<void>>(() => Promise.resolve()),
    ontoggle: vi.fn<(id: string, done: boolean) => Promise<void>>(() => Promise.resolve()),
    onremove: vi.fn<(id: string) => Promise<void>>(() => Promise.resolve()),
  };
}

describe('ChecklistForm (D-30, editor-less screens)', () => {
  it('adds an item and reuses the request id when the same text is retried', async () => {
    const h = handlers();
    h.onadd.mockRejectedValueOnce(new Error('network'));
    render(ChecklistForm, { idPrefix: 't', items: [], canEdit: true, ...h });
    const input = screen.getByLabelText('새 항목');
    await fireEvent.input(input, { target: { value: '자료 수집' } });
    await fireEvent.click(screen.getByRole('button', { name: '추가' }));
    await fireEvent.click(screen.getByRole('button', { name: '추가' }));
    expect(h.onadd).toHaveBeenCalledTimes(2);
    const first = h.onadd.mock.calls[0]?.[0]?.[0];
    const second = h.onadd.mock.calls[1]?.[0]?.[0];
    expect(first?.text).toBe('자료 수집');
    expect(second?.requestId).toBe(first?.requestId);
  });

  it('toggles completion through the checkbox', async () => {
    const h = handlers();
    render(ChecklistForm, {
      idPrefix: 't',
      items: [{ id: 'a', text: '초안 작성', done: false }],
      canEdit: true,
      ...h,
    });
    await fireEvent.click(screen.getByRole('checkbox', { name: '미완료 항목: 초안 작성' }));
    expect(h.ontoggle).toHaveBeenCalledWith('a', true);
  });

  it('is read-only without write access', () => {
    const h = handlers();
    render(ChecklistForm, {
      idPrefix: 't',
      items: [{ id: 'a', text: '초안 작성', done: false }],
      canEdit: false,
      ...h,
    });
    expect(screen.getByRole('checkbox', { name: '미완료 항목: 초안 작성' })).toBeDisabled();
    expect(screen.queryByLabelText('새 항목')).toBeNull();
  });
});

describe('Markdown checklist import', () => {
  it('extracts states and text and ignores other lines', () => {
    expect(
      parseMarkdownChecklist(
        '# 준비\n- [x] 자료 수집\n- [ ] 초안 작성\n* [X] 검토\n- 메모\n\n본문',
      ),
    ).toEqual([
      { text: '자료 수집', done: true },
      { text: '초안 작성', done: false },
      { text: '검토', done: true },
      { text: '메모', done: false },
    ]);
  });
});
