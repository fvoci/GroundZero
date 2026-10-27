import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import TiptapEditor from '$lib/features/documents/TiptapEditor.svelte';
import * as feedback from '$lib/features/feedback/feedback.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

// D-82: 파일 넣기의 완료는 토스트다. 편집기 아래 알림 영역은 올리는 중·실패만 보이고, 처음부터 그려 두고 글만 바꾼다
// (D-110(4)).
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
  vi.restoreAllMocks();
});

function region(): HTMLElement {
  const el = document.querySelector<HTMLElement>('p[aria-live="polite"].py-1');
  if (el === null) throw new Error('file status region');
  return el;
}

async function choose(file: File): Promise<void> {
  const input = screen.getByLabelText(m.documents_file_input());
  Object.defineProperty(input, 'files', { value: [file], configurable: true });
  await fireEvent.change(input);
}

it('reports an inserted file once as a toast and clears the uploading text', async () => {
  const done = vi.spyOn(feedback, 'notifyDone');
  let finish: (v: { href: string; name: string; image: boolean }) => void = () => undefined;
  const insertFile = vi.fn(
    () =>
      new Promise<{ href: string; name: string; image: boolean }>((resolve) => {
        finish = resolve;
      }),
  );
  render(TiptapEditor, {
    props: {
      content: { type: 'doc', content: [{ type: 'paragraph' }] },
      editable: true,
      label: '본문',
      insertFile,
    },
  });
  const status = region();
  expect(status.textContent).toBe('');

  await choose(new File(['x'], 'report.pdf', { type: 'application/pdf' }));
  expect(region()).toBe(status);
  expect(status).toHaveTextContent(m.documents_file_uploading({ name: 'report.pdf' }));

  finish({ href: '/files/1', name: 'report.pdf', image: false });
  await waitFor(() => {
    expect(done).toHaveBeenCalledOnce();
  });
  expect(done).toHaveBeenCalledWith(m.documents_file_inserted({ name: 'report.pdf' }));
  expect(status.textContent).toBe('');
  expect(screen.queryByText(m.documents_file_inserted({ name: 'report.pdf' }))).toBeNull();
});

it('keeps an insert failure in the page and shows no toast', async () => {
  const done = vi.spyOn(feedback, 'notifyDone');
  render(TiptapEditor, {
    props: {
      content: { type: 'doc', content: [{ type: 'paragraph' }] },
      editable: true,
      label: '본문',
      insertFile: () => Promise.reject(new Error('too large')),
    },
  });
  await choose(new File(['x'], 'big.pdf', { type: 'application/pdf' }));
  await waitFor(() => {
    expect(region()).toHaveTextContent(
      m.documents_file_insert_failed_reason({ reason: 'too large' }),
    );
  });
  expect(done).not.toHaveBeenCalled();
});
