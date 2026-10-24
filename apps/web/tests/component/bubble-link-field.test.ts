import { Editor } from '@tiptap/core';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import BubbleToolbar from '$lib/features/documents/BubbleToolbar.svelte';
import { schemaExtensions } from '$lib/features/documents/editor-schema';
import { setLocale } from '$lib/paraglide/runtime.js';

// 서식 막대의 링크 칸도 공통 Input으로 그린다(리뷰). 테두리·초점·잘못된 값 표시는 공통과 같고, 막대에 맞게 높이·폭·여백만
// 줄인다. 열면 주소 칸에 초점이 가고 Esc로 닫는다.
const editors: Editor[] = [];
beforeEach(async () => {
  await setLocale('ko', { reload: false });
});
afterEach(() => {
  for (const e of editors.splice(0)) e.destroy();
});

describe('formatting bar link field', () => {
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
