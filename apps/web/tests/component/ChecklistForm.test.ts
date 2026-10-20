import { fireEvent, render, screen } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import ChecklistForm from '$lib/features/checklists/ChecklistForm.svelte';
import type { NewChecklistEntry } from '$lib/features/checklists/checklist-item';
import { parseMarkdownChecklist } from '$lib/features/checklists/checklist-item';
import { setLocale } from '$lib/paraglide/runtime.js';

function handlers() {
  return {
    onadd: vi.fn<(items: NewChecklistEntry[]) => Promise<void>>(() => Promise.resolve()),
    ontoggle: vi.fn<(id: string, done: boolean) => Promise<void>>(() => Promise.resolve()),
    onrename: vi.fn<(id: string, text: string) => Promise<void>>(() => Promise.resolve()),
    onremove: vi.fn<(id: string) => Promise<void>>(() => Promise.resolve()),
  };
}

describe('ChecklistForm (D-30, editor-less screens)', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });

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

  // 감사 9번: 붙여넣기 확인의 응답을 잃은 뒤 줄을 고쳐 다시 보내면 새 요청 id여야 한다(같은 id면 서버가 처음 것을
  // 남기고 고친 내용을 조용히 버린다). 고치지 않고 다시 보내면 같은 id다.
  it('gives an edited paste line a new request id on retry', async () => {
    const h = handlers();
    h.onadd.mockRejectedValue(new Error('network'));
    render(ChecklistForm, { idPrefix: 't', items: [], canEdit: true, ...h });
    await fireEvent.click(screen.getByRole('button', { name: 'Markdown 붙여넣기' }));
    await fireEvent.input(screen.getByLabelText(/붙여넣을 체크리스트/), {
      target: { value: '- [ ] 자료 수집\n- [ ] 초안 작성' },
    });
    await fireEvent.click(screen.getByRole('button', { name: '항목으로 펼치기' }));
    const append = (): HTMLElement => screen.getByRole('button', { name: /덧붙이기/ });
    await fireEvent.click(append());
    await fireEvent.click(append());
    await fireEvent.input(screen.getByLabelText('항목 1 내용'), {
      target: { value: '자료 다시 수집' },
    });
    await fireEvent.click(append());
    const ids = h.onadd.mock.calls.map((c) => c[0].map((e) => e.requestId));
    expect(ids[1]).toEqual(ids[0]);
    expect(ids[2]?.[0]).not.toBe(ids[0]?.[0]);
    expect(ids[2]?.[1]).toBe(ids[0]?.[1]);
  });

  // 저장된 항목의 문구 고치기(기존 PUT, 완료 여부는 부모가 그대로 보낸다). 실패하면 입력을 남기고 알린다. Esc는 취소.
  it('renames an item, keeps the input when saving fails, and cancels with Escape', async () => {
    const h = handlers();
    h.onrename.mockRejectedValueOnce(new Error('network'));
    render(ChecklistForm, {
      idPrefix: 't',
      items: [{ id: 'a', text: '초안 작성', done: true }],
      canEdit: true,
      ...h,
    });
    await fireEvent.click(screen.getByRole('button', { name: '초안 작성 고치기' }));
    const input = screen.getByLabelText('초안 작성의 새 문구');
    await fireEvent.input(input, { target: { value: '  초안 다시 작성 ' } });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    expect(h.onrename).toHaveBeenCalledWith('a', '초안 다시 작성');
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    expect(screen.getByLabelText('초안 작성의 새 문구')).toHaveValue('  초안 다시 작성 ');

    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    expect(h.onrename).toHaveBeenCalledTimes(2);
    await screen.findByRole('button', { name: '초안 작성 고치기' });
    expect(screen.queryByLabelText('초안 작성의 새 문구')).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: '초안 작성 고치기' }));
    await fireEvent.keyDown(screen.getByLabelText('초안 작성의 새 문구'), { key: 'Escape' });
    expect(screen.queryByLabelText('초안 작성의 새 문구')).toBeNull();
    expect(h.onrename).toHaveBeenCalledTimes(2);
  });

  // 덧붙이기의 응답을 잃은 뒤 다시 펼쳐 보내도 같은 줄은 같은 요청 id다(서버가 이미 받았으면 중복되지 않는다).
  it('keeps paste request ids when the preview is expanded again after a failure', async () => {
    const h = handlers();
    h.onadd.mockRejectedValueOnce(new Error('network'));
    render(ChecklistForm, { idPrefix: 't', items: [], canEdit: true, ...h });
    await fireEvent.click(screen.getByRole('button', { name: 'Markdown 붙여넣기' }));
    await fireEvent.input(screen.getByLabelText(/붙여넣을 체크리스트/), {
      target: { value: '- [ ] 자료 수집\n- [x] 초안 작성' },
    });
    const expand = (): HTMLElement => screen.getByRole('button', { name: '항목으로 펼치기' });
    await fireEvent.click(expand());
    await fireEvent.click(screen.getByRole('button', { name: /덧붙이기/ }));
    await fireEvent.click(expand());
    await fireEvent.click(screen.getByRole('button', { name: /덧붙이기/ }));
    const ids = h.onadd.mock.calls.map((c) => c[0].map((e) => e.requestId));
    expect(ids).toHaveLength(2);
    expect(ids[1]).toEqual(ids[0]);
  });

  // 응답을 잃은 뒤 줄 순서를 바꿔 다시 펼쳐도 같은 줄은 같은 요청 id다(자리로 붙이면 이미 저장된 줄이 새 id로 중복됐다).
  it('keeps paste request ids per line content when lines are reordered', async () => {
    const h = handlers();
    h.onadd.mockRejectedValueOnce(new Error('network'));
    render(ChecklistForm, { idPrefix: 't', items: [], canEdit: true, ...h });
    await fireEvent.click(screen.getByRole('button', { name: 'Markdown 붙여넣기' }));
    const source = (): HTMLElement => screen.getByLabelText(/붙여넣을 체크리스트/);
    const expand = (): HTMLElement => screen.getByRole('button', { name: '항목으로 펼치기' });
    await fireEvent.input(source(), { target: { value: '- [ ] 자료 수집\n- [ ] 초안 작성' } });
    await fireEvent.click(expand());
    await fireEvent.click(screen.getByRole('button', { name: /덧붙이기/ }));
    await fireEvent.input(source(), { target: { value: '- [ ] 초안 작성\n- [ ] 자료 수집' } });
    await fireEvent.click(expand());
    await fireEvent.click(screen.getByRole('button', { name: /덧붙이기/ }));
    const byText = (call: number): Record<string, string> =>
      Object.fromEntries((h.onadd.mock.calls[call]?.[0] ?? []).map((e) => [e.text, e.requestId]));
    expect(h.onadd).toHaveBeenCalledTimes(2);
    expect(byText(1)).toEqual(byText(0));
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
    expect(screen.queryByRole('button', { name: '초안 작성 고치기' })).toBeNull();
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
