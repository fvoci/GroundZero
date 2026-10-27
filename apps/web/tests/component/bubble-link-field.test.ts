import { Editor } from '@tiptap/core';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { formDraftTabKey, writeFormDraftAt } from '$lib/features/common/form-draft';
import BubbleToolbar from '$lib/features/documents/BubbleToolbar.svelte';
import { schemaExtensions } from '$lib/features/documents/editor-schema';
import type { LinkDraftControl } from '$lib/features/documents/link-draft-control';
import TiptapEditor from '$lib/features/documents/TiptapEditor.svelte';
import { m } from '$lib/paraglide/messages.js';
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
    // 제안은 편집기 본문 위(TiptapEditor)가 그린다. 여기서는 막대가 넘기는 연결로 바로 되살린다.
    let control: LinkDraftControl | undefined;
    render(BubbleToolbar, {
      props: {
        editor,
        version: 2,
        draftName: 'doc-a',
        onlinkcontrol: (value: LinkDraftControl) => {
          control = value;
        },
      },
    });
    const [offer] = control?.offers() ?? [];
    if (control === undefined || offer === undefined) throw new Error('missing link offer');
    control.restore(offer);
    expect(await screen.findByRole('textbox', { name: '링크 주소' })).toHaveValue(
      'https://example.test/',
    );
    expect(editor.getHTML()).toBe('<p>first second</p>');
    const unload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(true);
    await fireEvent.click(screen.getByRole('button', { name: '적용' }));
    expect(editor.getHTML()).toMatch(/first <a[^>]*>second<\/a>/);
  });

  // R5 P2 F-6(⑱): 남긴 링크 주소의 제안은 말풍선이 아니라 편집기 본문 위에 보인다(말풍선은 글자를 골라야 나타난다).
  // 고른 글자 없이 되살리면 문서는 그대로 두고 글자를 고르라고 안내한다.
  it('offers a saved link address above the editor body and asks for a selection', async () => {
    Range.prototype.getBoundingClientRect = (): DOMRect => new DOMRect(0, 0, 0, 0);
    Range.prototype.getClientRects = (): DOMRectList => Object.assign([], { item: () => null });
    writeFormDraftAt(formDraftTabKey('editor-link:doc-a', 'user-a'), {
      savedAt: '2026-10-27T00:00:00Z',
      input: 'https://example.test/',
    });
    const changed = vi.fn();
    render(TiptapEditor, {
      props: {
        content: {
          type: 'doc',
          content: [{ type: 'paragraph', content: [{ type: 'text', text: '본문' }] }],
        },
        editable: true,
        label: '본문',
        draftName: 'doc-a',
        onchange: changed,
      },
    });
    const offer = await screen.findByTestId('form-draft-offer');
    expect(screen.getByTestId('bubble-toolbar')).not.toContainElement(offer);
    await fireEvent.click(within(offer).getByRole('button', { name: m.common_draft_restore() }));
    expect(await screen.findByText(m.documents_link_select_text())).toBeInTheDocument();
    expect(screen.queryByTestId('form-draft-offer')).toBeNull();
    expect(changed).not.toHaveBeenCalled();
    const unload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(true);
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
