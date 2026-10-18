import { Editor } from '@tiptap/core';
import type { JSONContent } from '@tiptap/core';
import { afterEach, describe, expect, it } from 'vitest';

import { blockAt, runBlockOp } from '$lib/features/documents/block-ops';
import { schemaExtensions } from '$lib/features/documents/editor-schema';

const editors: Editor[] = [];
afterEach(() => {
  for (const e of editors.splice(0)) e.destroy();
});

function make(content: JSONContent): Editor {
  const editor = new Editor({ extensions: schemaExtensions(), content });
  editors.push(editor);
  return editor;
}

const p = (text: string): JSONContent => ({
  type: 'paragraph',
  content: [{ type: 'text', text }],
});
const task = (text: string, checked = false): JSONContent => ({
  type: 'taskItem',
  attrs: { checked },
  content: [p(text)],
});

/** 문서 바로 아래 블록(또는 목록 항목)의 글자를 차례로. */
function texts(editor: Editor): string[] {
  const out: string[] = [];
  editor.state.doc.forEach((node) => {
    if (node.type.name === 'taskList' || node.type.name === 'bulletList')
      node.forEach((item) => {
        out.push(
          `${item.type.name === 'taskItem' && item.attrs.checked === true ? '[x]' : ''}${item.textContent}`,
        );
      });
    else out.push(node.textContent);
  });
  return out;
}

/** 글자 안쪽 위치(블록을 고르는 데 쓴다). */
function posOf(editor: Editor, text: string): number {
  let found = -1;
  editor.state.doc.descendants((node, pos) => {
    if (found < 0 && node.isText && node.text === text) found = pos + 1;
  });
  return found;
}

describe('block operations (single ProseMirror transactions)', () => {
  it('moves top-level blocks up and down and stops at the edges', () => {
    const e = make({ type: 'doc', content: [p('A'), p('B'), p('C')] });
    expect(runBlockOp(e, 'up', posOf(e, 'B'))).toBe(true);
    expect(texts(e)).toEqual(['B', 'A', 'C']);
    expect(runBlockOp(e, 'up', posOf(e, 'B'))).toBe(false);
    expect(runBlockOp(e, 'down', posOf(e, 'A'))).toBe(true);
    expect(texts(e)).toEqual(['B', 'C', 'A']);
    // 옮긴 블록 안에 커서가 남는다.
    expect(e.state.selection.$from.parent.textContent).toBe('A');
  });

  it('moves checklist items inside their list and keeps their checked state', () => {
    const e = make({
      type: 'doc',
      content: [{ type: 'taskList', content: [task('one'), task('two', true)] }],
    });
    expect(blockAt(e.state.doc, posOf(e, 'two'))?.node.type.name).toBe('taskItem');
    runBlockOp(e, 'up', posOf(e, 'two'));
    // 문서 끝 목록 뒤의 빈 문단은 StarterKit의 끝 문단(TrailingNode)이다.
    expect(texts(e)).toEqual(['[x]two', 'one', '']);
  });

  it('duplicates a block below without touching the original (references stay links)', () => {
    const e = make({
      type: 'doc',
      content: [
        {
          type: 'taskList',
          content: [
            {
              type: 'taskItem',
              attrs: { checked: true },
              content: [
                {
                  type: 'paragraph',
                  content: [
                    { type: 'text', text: 'see ' },
                    {
                      type: 'reference',
                      attrs: { kind: 'task', id: '01a00000-0000-7000-8000-000000000001' },
                    },
                  ],
                },
              ],
            },
          ],
        },
      ],
    });
    const before = JSON.stringify(e.getJSON());
    runBlockOp(e, 'duplicate', posOf(e, 'see '));
    const list = e.getJSON().content[0]?.content ?? [];
    expect(list).toHaveLength(2);
    expect(list[0]).toEqual(list[1]);
    // 원본 항목은 그대로(같은 위치·같은 속성).
    expect(
      JSON.stringify({ type: 'doc', content: [{ type: 'taskList', content: [list[0]] }] }),
    ).toBe(before);
  });

  it('deletes a block; the last item removes its list; the last block leaves an empty paragraph', () => {
    const e = make({
      type: 'doc',
      content: [p('keep'), { type: 'taskList', content: [task('only')] }],
    });
    runBlockOp(e, 'delete', posOf(e, 'only'));
    expect(e.getJSON().content.map((n) => n.type)).toEqual(['paragraph']);
    runBlockOp(e, 'delete', posOf(e, 'keep'));
    expect(e.getJSON()).toEqual({ type: 'doc', content: [{ type: 'paragraph' }] });
  });

  it('inserts an empty block below; inside a checklist it is an unchecked item', () => {
    const e = make({
      type: 'doc',
      content: [{ type: 'taskList', content: [task('done', true)] }, p('end')],
    });
    runBlockOp(e, 'below', posOf(e, 'done'));
    expect(texts(e)).toEqual(['[x]done', '', 'end']);
    expect(e.state.selection.$from.node(-1).type.name).toBe('taskItem');
    const e2 = make({ type: 'doc', content: [p('x')] });
    runBlockOp(e2, 'below', posOf(e2, 'x'));
    expect(texts(e2)).toEqual(['x', '']);
  });

  it('undoes a move in one step', () => {
    const e = make({ type: 'doc', content: [p('A'), p('B')] });
    runBlockOp(e, 'down', posOf(e, 'A'));
    expect(texts(e)).toEqual(['B', 'A']);
    e.commands.undo();
    expect(texts(e)).toEqual(['A', 'B']);
  });

  it('does nothing when the editor is read-only', () => {
    const e = make({ type: 'doc', content: [p('A'), p('B')] });
    e.setEditable(false);
    expect(runBlockOp(e, 'down', posOf(e, 'A'))).toBe(false);
    expect(texts(e)).toEqual(['A', 'B']);
  });
});
