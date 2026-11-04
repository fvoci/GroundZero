import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import ChecklistForm from '$lib/features/checklists/ChecklistForm.svelte';
import type { NewChecklistEntry } from '$lib/features/checklists/checklist-item';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

// #289: 체크리스트 입력은 `canEdit`일 때만 보인다. 숨은 뒤에도 부모의 이탈 경고에 남으면 안 된다(D-27). 화면 전용
// 입력은 서버 값(빈 칸)으로 돌리고, 남기는 초안이면 지우지 않고 되살리기 제안으로 옮긴다.

const items = [{ id: 'a', text: '초안 작성', done: false }];

function props(canEdit: boolean, extra: Record<string, unknown> = {}) {
  return {
    idPrefix: 'hidden',
    items,
    canEdit,
    onadd: vi.fn<(entries: NewChecklistEntry[]) => Promise<void>>(() => Promise.resolve()),
    ontoggle: vi.fn(() => Promise.resolve()),
    onrename: vi.fn(() => Promise.resolve()),
    onremove: vi.fn(() => Promise.resolve()),
    ...extra,
  };
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'user-a';
  localStorage.clear();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  me.userId = '';
  localStorage.clear();
});

it('drops hidden new item, paste and rename input when editing ends', async () => {
  const unsaved = vi.fn();
  const first = props(true, { onunsaved: unsaved });
  const view = render(ChecklistForm, first);
  await fireEvent.input(screen.getByLabelText(m.checklists_new_item()), {
    target: { value: '숨을 새 항목' },
  });
  await fireEvent.click(screen.getByRole('button', { name: m.checklists_paste_markdown() }));
  await fireEvent.input(screen.getByLabelText(m.checklists_paste_label()), {
    target: { value: '- [ ] 붙여 넣은 줄' },
  });
  await fireEvent.click(
    screen.getByRole('button', { name: m.checklists_edit_aria({ text: '초안 작성' }) }),
  );
  await fireEvent.input(screen.getByLabelText(m.checklists_edit_label({ text: '초안 작성' })), {
    target: { value: '고친 문구' },
  });
  expect(unsaved).toHaveBeenLastCalledWith(true);
  await view.rerender({ ...first, canEdit: false });
  expect(screen.queryByLabelText(m.checklists_new_item())).toBeNull();
  await waitFor(() => {
    expect(unsaved).toHaveBeenLastCalledWith(false);
  });
  await view.rerender({ ...first, canEdit: true });
  expect(screen.getByLabelText(m.checklists_new_item())).toHaveValue('');
  expect(screen.queryByLabelText(m.checklists_paste_label())).toBeNull();
  expect(screen.queryByLabelText(m.checklists_edit_label({ text: '초안 작성' }))).toBeNull();
  expect(first.onadd).not.toHaveBeenCalled();
  expect(first.onrename).not.toHaveBeenCalled();
});

it('keeps a sent item locked until its response, then drops the hidden input', async () => {
  let reject: (e: Error) => void = () => undefined;
  const unsaved = vi.fn();
  const first = props(true, {
    onunsaved: unsaved,
    onadd: vi.fn(
      () =>
        new Promise<void>((_, fail) => {
          reject = fail;
        }),
    ),
  });
  const view = render(ChecklistForm, first);
  await fireEvent.input(screen.getByLabelText(m.checklists_new_item()), {
    target: { value: '보내는 항목' },
  });
  await fireEvent.click(screen.getByRole('button', { name: m.checklists_add() }));
  await view.rerender({ ...first, canEdit: false });
  expect(unsaved).toHaveBeenLastCalledWith(true);
  reject(new Error('forbidden'));
  await waitFor(() => {
    expect(unsaved).toHaveBeenLastCalledWith(false);
  });
  await view.rerender({ ...first, canEdit: true });
  expect(screen.getByLabelText(m.checklists_new_item())).toHaveValue('');
});

it('moves a kept checklist draft into an offer instead of deleting it', async () => {
  const unsaved = vi.fn();
  const first = props(true, { onunsaved: unsaved, preserveDraft: true });
  const view = render(ChecklistForm, first);
  await fireEvent.input(screen.getByLabelText(m.checklists_new_item()), {
    target: { value: '남길 새 항목' },
  });
  await view.rerender({ ...first, canEdit: false });
  await waitFor(() => {
    expect(unsaved).toHaveBeenLastCalledWith(false);
  });
  const offer = await screen.findByTestId('checklist-new-item-draft-offer');
  expect(offer).toHaveTextContent('남길 새 항목');
  expect(offer).toHaveTextContent(m.common_draft_read_only());
  await view.rerender({ ...first, canEdit: true });
  expect(screen.getByLabelText(m.checklists_new_item())).toHaveValue('');
  await fireEvent.click(screen.getByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.checklists_new_item())).toHaveValue('남길 새 항목');
});
