import { fireEvent, render, screen } from '@testing-library/svelte';
import type { Editor } from '@tiptap/core';
import { tick } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocNode } from '$lib/api/client';
import TiptapEditor from '$lib/features/documents/TiptapEditor.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// `/` 메뉴의 "구분선"을 실제 편집기에서 골라, 뒤 블록(체크리스트)이 지워지지 않는지 본다. `insertDivider` 단독 시험과 달리
// 편집기의 메뉴 항목 연결(`TiptapEditor.svelte`의 divider 항목)을 거친다.
const body: DocNode = {
  type: 'doc',
  content: [
    { type: 'paragraph', content: [{ type: 'text', text: '앞' }] },
    { type: 'paragraph' },
    {
      type: 'taskList',
      content: [
        {
          type: 'taskItem',
          attrs: { checked: false },
          content: [{ type: 'paragraph', content: [{ type: 'text', text: '할 일' }] }],
        },
      ],
    },
  ],
};

describe('slash menu divider in the editor', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    // jsdom에는 배치 계산(Range 사각형)이 없어 메뉴 위치 계산이 쓰는 값을 빈 사각형으로 채운다.
    Range.prototype.getBoundingClientRect = (): DOMRect => new DOMRect(0, 0, 0, 0);
    const none: DOMRectList = Object.assign([], { item: (): DOMRect | null => null });
    Range.prototype.getClientRects = (): DOMRectList => none;
    Element.prototype.scrollIntoView = (): void => undefined;
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('keeps the checklist below and puts the next typing in a new paragraph', async () => {
    const onchange = vi.fn<(doc: DocNode) => void>();
    render(TiptapEditor, { props: { content: body, editable: true, label: '본문', onchange } });
    await tick();
    const dom = document.querySelector<HTMLElement & { editor?: Editor }>('.ProseMirror');
    const editor = dom?.editor;
    if (dom === null || editor === undefined) throw new Error('editor');
    // "앞" 아래 빈 문단에서 `/구분`을 입력한다(메뉴는 줄 처음이나 띄어쓰기 뒤의 `/`에서 열린다).
    editor.commands.setTextSelection(4);
    editor.commands.insertContent('/구분');
    await tick();
    const option = await screen.findByRole('option', { name: /구분선/ });
    expect(option).toBeTruthy();
    await fireEvent.keyDown(dom, { key: 'Enter' });
    await tick();
    editor.commands.insertContent('뒤');
    await tick();
    const blocks = editor.getJSON().content;
    expect(blocks.map((b) => b.type).slice(0, 4)).toEqual([
      'paragraph',
      'horizontalRule',
      'paragraph',
      'taskList',
    ]);
    expect(editor.state.doc.child(2).textContent).toBe('뒤');
    // 구분선 아래 체크리스트는 그대로다(전에는 다음 입력이 체크 항목을 덮어썼다).
    expect(editor.state.doc.child(3).textContent).toBe('할 일');
    expect(onchange).toHaveBeenCalled();
  });
});
