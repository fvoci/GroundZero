import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import ChecklistForm from '$lib/features/checklists/ChecklistForm.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

function handlers() {
  return {
    onadd: vi
      .fn<
        (items: { text: string; done: boolean; requestId: string }[], user: string) => Promise<void>
      >()
      .mockResolvedValue(undefined),
    ontoggle: vi.fn().mockResolvedValue(undefined),
    onrename: vi.fn().mockResolvedValue(undefined),
    onremove: vi.fn().mockResolvedValue(undefined),
  };
}
const base = {
  idPrefix: 'task-t1',
  items: [{ id: 'c1', text: '원래 문구', done: false }],
  canEdit: true,
};

describe('checklist drafts', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    localStorage.clear();
    me.userId = 'user-a';
  });
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    localStorage.clear();
    me.userId = '';
  });

  it('offers new item input after remount and reuses the failed request id', async () => {
    const onadd = vi.fn().mockRejectedValue(new Error('offline'));
    const props = {
      idPrefix: 'task-t1',
      items: [],
      canEdit: true,
      onadd,
      ontoggle: vi.fn(),
      onrename: vi.fn(),
      onremove: vi.fn(),
    };
    const view = render(ChecklistForm, props);
    await fireEvent.input(screen.getByLabelText('새 항목'), { target: { value: '자료 수집' } });
    await fireEvent.click(screen.getByRole('button', { name: '추가' }));
    await screen.findByRole('alert');
    view.unmount();
    render(ChecklistForm, props);
    expect(screen.getByLabelText('새 항목')).toHaveValue('');
    expect(await screen.findByTestId('checklist-new-item-draft-offer')).toHaveTextContent(
      '자료 수집',
    );
    await fireEvent.click(screen.getByRole('button', { name: '입력 되살리기' }));
    expect(onadd).toHaveBeenCalledTimes(1);
    onadd.mockResolvedValueOnce(undefined);
    await fireEvent.click(screen.getByRole('button', { name: '추가' }));
    await waitFor(() => {
      expect(onadd).toHaveBeenCalledTimes(2);
    });
    expect(onadd.mock.calls[1]?.[0]).toEqual(onadd.mock.calls[0]?.[0]);
    expect(onadd.mock.calls[1]?.[1]).toBe('user-a');
  });

  it('preserves rename, paste source, edited preview, completion and retry ids together', async () => {
    const h = handlers();
    h.onadd.mockRejectedValueOnce(new Error('offline'));
    const onunsaved = vi.fn();
    const view = render(ChecklistForm, { ...base, ...h, onunsaved });
    await fireEvent.click(screen.getByRole('button', { name: '원래 문구 고치기' }));
    await fireEvent.input(screen.getByLabelText('원래 문구의 새 문구'), {
      target: { value: '고치는 중' },
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Markdown 붙여넣기' }));
    await fireEvent.input(screen.getByLabelText(/붙여넣을 체크리스트/), {
      target: { value: '- [ ] 원문\n- [x] 둘' },
    });
    await fireEvent.click(screen.getByRole('button', { name: '항목으로 펼치기' }));
    await fireEvent.input(screen.getByLabelText('항목 1 내용'), {
      target: { value: '미리보기에서 고친 내용' },
    });
    await fireEvent.click(
      screen.getByRole('checkbox', { name: '미리보기에서 고친 내용 완료 여부' }),
    );
    await fireEvent.click(screen.getByRole('button', { name: '2개 덧붙이기' }));
    await screen.findByRole('alert');
    expect(onunsaved).toHaveBeenLastCalledWith(true);
    view.unmount();
    render(ChecklistForm, { ...base, ...h, onunsaved });
    await screen.findByTestId('checklist-new-item-draft-offer');
    expect(screen.queryByLabelText('원래 문구의 새 문구')).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: '입력 되살리기' }));
    expect(screen.getByLabelText('원래 문구의 새 문구')).toHaveValue('고치는 중');
    expect(screen.getByLabelText(/붙여넣을 체크리스트/)).toHaveValue('- [ ] 원문\n- [x] 둘');
    expect(screen.getByLabelText('항목 1 내용')).toHaveValue('미리보기에서 고친 내용');
    expect(
      screen.getByRole('checkbox', { name: '미리보기에서 고친 내용 완료 여부' }),
    ).toBeChecked();
    expect(h.onadd).toHaveBeenCalledTimes(1);
    await fireEvent.click(screen.getByRole('button', { name: '2개 덧붙이기' }));
    await waitFor(() => {
      expect(h.onadd).toHaveBeenCalledTimes(2);
    });
    expect(h.onadd.mock.calls[1]).toEqual(h.onadd.mock.calls[0]);
    expect(screen.getByLabelText('원래 문구의 새 문구')).toHaveValue('고치는 중');
    expect(onunsaved).toHaveBeenLastCalledWith(true);
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(h.onrename).toHaveBeenCalledWith('c1', '고치는 중', 'user-a');
    });
    await waitFor(() => {
      expect(onunsaved).toHaveBeenLastCalledWith(false);
    });
    expect(
      Object.keys(localStorage).filter((key) => key.includes('form-draft:checklist:')),
    ).toEqual([]);
  });

  it('keeps the original account input and ignores its late success after switching accounts', async () => {
    const h = handlers();
    let resolve: () => void = () => undefined;
    h.onadd.mockImplementationOnce(
      () =>
        new Promise<void>((done) => {
          resolve = done;
        }),
    );
    render(ChecklistForm, { ...base, ...h });
    await fireEvent.input(screen.getByLabelText('새 항목'), { target: { value: 'A 입력' } });
    await fireEvent.click(screen.getByRole('button', { name: '추가' }));
    me.userId = 'user-b';
    await waitFor(() => {
      expect(screen.getByLabelText('새 항목')).toHaveValue('');
    });
    expect(screen.queryByTestId('checklist-new-item-draft-offer')).toBeNull();
    expect(Object.keys(localStorage).filter((key) => key.includes('user-b'))).toEqual([]);
    await fireEvent.input(screen.getByLabelText('새 항목'), { target: { value: 'B 입력' } });
    resolve();
    await waitFor(() => {
      expect(screen.getByLabelText('새 항목')).toHaveValue('B 입력');
    });
    me.userId = 'user-a';
    expect(await screen.findByTestId('checklist-new-item-draft-offer')).toHaveTextContent('A 입력');
    await fireEvent.click(screen.getByRole('button', { name: '입력 되살리기' }));
    expect(screen.getByLabelText('새 항목')).toHaveValue('A 입력');
    expect(h.onadd).toHaveBeenCalledTimes(1);
  });

  it('retains input through permission withdrawal and lets the user discard the saved offer', async () => {
    const h = handlers();
    const onunsaved = vi.fn();
    const view = render(ChecklistForm, { ...base, ...h, onunsaved });
    await fireEvent.input(screen.getByLabelText('새 항목'), {
      target: { value: '권한 철회 전 입력' },
    });
    await view.rerender({ ...base, ...h, onunsaved, canEdit: false });
    expect(screen.queryByLabelText('새 항목')).toBeNull();
    expect(onunsaved).toHaveBeenLastCalledWith(true);
    view.unmount();
    const next = render(ChecklistForm, { ...base, ...h, canEdit: false });
    // R5 F-11: 고칠 수 없는 동안은 되살리기 대신 읽기 전용 안내와 버리기만 보인다.
    const offer = await screen.findByTestId('checklist-new-item-draft-offer');
    expect(offer).toHaveTextContent('권한 철회 전 입력');
    expect(within(offer).queryByRole('button', { name: m.common_draft_restore() })).toBeNull();
    expect(offer).toHaveTextContent(m.common_draft_read_only());
    await next.rerender({ ...base, ...h });
    expect(
      within(screen.getByTestId('checklist-new-item-draft-offer')).getByRole('button', {
        name: m.common_draft_restore(),
      }),
    ).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: '남긴 입력 버리기' }));
    expect(screen.queryByTestId('checklist-new-item-draft-offer')).toBeNull();
    expect(h.onadd).not.toHaveBeenCalled();
    expect(screen.getByLabelText('새 항목')).toHaveValue('');
  });

  it('keeps working without local storage and reports unsaved changes', async () => {
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('full');
    });
    const h = handlers();
    const onunsaved = vi.fn();
    render(ChecklistForm, { ...base, ...h, onunsaved });
    await fireEvent.input(screen.getByLabelText('새 항목'), { target: { value: '저장소 없이' } });
    expect(onunsaved).toHaveBeenLastCalledWith(true);
    await fireEvent.click(screen.getByRole('button', { name: '추가' }));
    await waitFor(() => {
      expect(h.onadd).toHaveBeenCalledTimes(1);
    });
    await waitFor(() => {
      expect(screen.getByLabelText('새 항목')).toHaveValue('');
    });
    expect(onunsaved).toHaveBeenLastCalledWith(false);
  });

  it('keeps another tab input intact when this tab saves', async () => {
    const h = handlers();
    sessionStorage.setItem('fvoci:tab', 'checklist-first');
    const first = render(ChecklistForm, { ...base, ...h });
    const a = within(first.container);
    sessionStorage.setItem('fvoci:tab', 'checklist-second');
    const second = render(ChecklistForm, { ...base, ...h });
    const b = within(second.container);
    // 같은 문서에 두 탭을 흉내 내므로 id/label은 중복된다. 각 컨테이너의 입력을 직접 고른다.
    await fireEvent.input(a.getByRole('textbox'), { target: { value: '첫 탭 항목' } });
    await fireEvent.input(b.getByRole('textbox'), { target: { value: '둘째 탭 항목' } });
    const key = Object.keys(localStorage).find((candidate) =>
      candidate.endsWith(':tab:checklist-second'),
    );
    if (key === undefined) throw new Error('second tab draft');
    const saved = localStorage.getItem(key);
    await fireEvent.click(a.getByRole('button', { name: '추가' }));
    await waitFor(() => {
      expect(a.getByRole('textbox')).toHaveValue('');
    });
    expect(b.getByRole('textbox')).toHaveValue('둘째 탭 항목');
    expect(localStorage.getItem(key)).toBe(saved);
  });
});
