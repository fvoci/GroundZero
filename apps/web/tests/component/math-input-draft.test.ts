import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import TiptapEditor from '$lib/features/documents/TiptapEditor.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  localStorage.clear();
  me.userId = 'user-a';
  Range.prototype.getBoundingClientRect = (): DOMRect => new DOMRect(0, 0, 0, 0);
  Range.prototype.getClientRects = (): DOMRectList => Object.assign([], { item: () => null });
});
afterEach(() => {
  cleanup();
  me.userId = '';
  localStorage.clear();
});

it('restores unapplied LaTeX as input without changing the document', async () => {
  const changed = vi.fn();
  const props = {
    content: { type: 'doc', content: [{ type: 'paragraph' }] },
    editable: true,
    label: '본문',
    draftName: 'doc-a',
    onchange: changed,
  };
  render(TiptapEditor, { props });
  await fireEvent.click(screen.getByRole('button', { name: m.documents_math() }));
  await fireEvent.input(screen.getByLabelText(m.documents_math_latex()), {
    target: { value: 'x^2 + y^2' },
  });
  cleanup();
  changed.mockClear();
  render(TiptapEditor, { props });
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.documents_math_latex())).toHaveValue('x^2 + y^2');
  expect(changed).not.toHaveBeenCalled();
  const unload = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(unload);
  expect(unload.defaultPrevented).toBe(true);
  await fireEvent.click(screen.getByRole('button', { name: m.common_cancel() }));
  await screen.findByRole('alertdialog');
  await fireEvent.click(screen.getByRole('button', { name: m.common_discard_keep() }));
  expect(screen.getByLabelText(m.documents_math_latex())).toHaveValue('x^2 + y^2');
});

// R5 B 최종 C-6(D3B-F-2와 같은 규칙): 수식 폼은 편집할 수 있을 때만 그린다. 연 채로 편집할 수 없게 되면(실시간 연결
// 중단·권한 변화) 보이지 않는 입력으로 이탈 확인을 띄우지 않고, 이 탭에 남긴 입력은 지우지 않아 다시 편집할 수 있게 되면
// 되살리기 제안으로 고르게 한다.
it('stops asking for a formula that can no longer be edited, and keeps it for later', async () => {
  const props = {
    content: { type: 'doc', content: [{ type: 'paragraph' }] },
    editable: true,
    label: '본문',
    draftName: 'doc-a',
    onchange: vi.fn(),
  };
  const view = render(TiptapEditor, { props });
  await fireEvent.click(screen.getByRole('button', { name: m.documents_math() }));
  await fireEvent.input(screen.getByLabelText(m.documents_math_latex()), {
    target: { value: 'e^{i\\pi}' },
  });
  const stored = (): string[] =>
    Object.keys(localStorage).filter((key) => key.includes('editor-math:doc-a'));
  expect(stored()).toHaveLength(1);

  await view.rerender({ ...props, editable: false });
  expect(screen.queryByLabelText(m.documents_math_latex())).toBeNull();
  const unload = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(unload);
  expect(unload.defaultPrevented).toBe(false);
  // 남긴 입력은 그대로다.
  expect(stored()).toHaveLength(1);
  expect(localStorage.getItem(stored()[0] ?? '')).toContain('e^{i\\\\pi}');

  await view.rerender({ ...props, editable: true });
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.documents_math_latex())).toHaveValue('e^{i\\pi}');
});
