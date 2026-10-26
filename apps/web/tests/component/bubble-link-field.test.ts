import { Editor } from '@tiptap/core';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import BubbleToolbar from '$lib/features/documents/BubbleToolbar.svelte';
import { schemaExtensions } from '$lib/features/documents/editor-schema';
import { setLocale } from '$lib/paraglide/runtime.js';
import { me } from '$lib/me.svelte';

// 서식 막대의 링크 칸도 공통 Input으로 그린다(리뷰). 테두리·초점·잘못된 값 표시는 공통과 같고, 막대에 맞게 높이·폭·여백만
// 줄인다. 열면 주소 칸에 초점이 가고 Esc로 닫는다.
const editors: Editor[] = [];
beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'user-a';
  localStorage.clear();
});
afterEach(() => {
  cleanup();
  for (const e of editors.splice(0)) e.destroy();
  me.userId = '';
  localStorage.clear();
});

describe('formatting bar link field', () => {
  it('restores only the URL text and applies it only to a newly selected range', async () => {
    const editor = new Editor({ extensions: schemaExtensions(), content: '<p>first second</p>' });
    editors.push(editor);
    editor.commands.setTextSelection({ from: 1, to: 6 });
    render(BubbleToolbar, { props: { editor, version: 1, draftName: 'doc-a' } });
    await fireEvent.click(screen.getByRole('button', { name: '링크' }));
    await fireEvent.input(screen.getByRole('textbox', { name: '링크 주소' }), {
      target: { value: 'https://example.test/' },
    });
    cleanup();
    editor.commands.setTextSelection({ from: 7, to: 13 });
    render(BubbleToolbar, { props: { editor, version: 2, draftName: 'doc-a' } });
    await fireEvent.click(await screen.findByRole('button', { name: '입력 되살리기' }));
    expect(screen.getByRole('textbox', { name: '링크 주소' })).toHaveValue('https://example.test/');
    expect(editor.getHTML()).toBe('<p>first second</p>');
    const unload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(true);
    await fireEvent.click(screen.getByRole('button', { name: '적용' }));
    expect(editor.getHTML()).toMatch(/first <a[^>]*>second<\/a>/);
  });

  it('uses the shared Input, focuses it on open and closes on Escape', async () => {
    const editor = new Editor({
      extensions: schemaExtensions(),
      content: '<p>링크를 걸 글</p>',
    });
    editors.push(editor);
    editor.commands.setTextSelection({ from: 1, to: 4 });
    render(BubbleToolbar, { props: { editor, version: 1 } });

    await fireEvent.click(screen.getByRole('button', { name: '링크' }));
    const field = await screen.findByRole('textbox', { name: '링크 주소' });
    expect(field).toHaveAttribute('data-slot', 'input');
    expect(field).toHaveClass('border-input-border', 'h-7');
    expect(field).toHaveAttribute('placeholder', 'https://');
    await waitFor(() => {
      expect(field).toHaveFocus();
    });

    await fireEvent.keyDown(field, { key: 'Escape' });
    await waitFor(() => {
      expect(screen.queryByRole('textbox', { name: '링크 주소' })).toBeNull();
    });
  });
});
