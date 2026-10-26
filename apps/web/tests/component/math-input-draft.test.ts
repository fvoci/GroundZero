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
