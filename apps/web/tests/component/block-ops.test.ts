import { Editor } from '@tiptap/core';
import type { JSONContent } from '@tiptap/core';
import { afterEach, describe, expect, it } from 'vitest';

import {
  blockAt,
  blockAtHit,
  blockStartingAt,
  mapBlockStart,
  mapBlockStartThrough,
  runBlockOp,
} from '$lib/features/documents/block-ops';
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

describe('leaf blocks (divider, image, block math) are addressed by their start', () => {
  it('finds the divider itself, not the block after it, and deletes only it', () => {
    const e = make({ type: 'doc', content: [p('a'), { type: 'horizontalRule' }, p('b')] });
    const hrFrom = e.state.doc.child(0).nodeSize;
    expect(blockStartingAt(e.state.doc, hrFrom)?.node.type.name).toBe('horizontalRule');
    // 화면에서 구분선을 가리키면(inside = 구분선 시작) 구분선이다.
    expect(blockAtHit(e.state.doc, { pos: hrFrom, inside: hrFrom })?.node.type.name).toBe(
      'horizontalRule',
    );
    const block = blockStartingAt(e.state.doc, hrFrom);
    if (block === undefined) throw new Error('block');
    expect(runBlockOp(e, 'delete', block)).toBe(true);
    expect(texts(e)).toEqual(['a', 'b']);
  });

  it('moves a divider without touching its neighbours', () => {
    const e = make({ type: 'doc', content: [p('a'), { type: 'horizontalRule' }, p('b')] });
    const block = blockStartingAt(e.state.doc, e.state.doc.child(0).nodeSize);
    if (block === undefined) throw new Error('block');
    runBlockOp(e, 'up', block);
    expect(e.getJSON().content.map((n) => n.type)).toEqual([
      'horizontalRule',
      'paragraph',
      'paragraph',
    ]);
    expect(texts(e)).toEqual(['', 'a', 'b']);
  });

  it('a block start inside a list resolves to the list item', () => {
    const e = make({
      type: 'doc',
      content: [{ type: 'taskList', content: [task('x'), task('y')] }],
    });
    const itemFrom = blockAt(e.state.doc, posOf(e, 'y'))?.from;
    if (itemFrom === undefined) throw new Error('item');
    expect(blockStartingAt(e.state.doc, itemFrom)?.node.textContent).toBe('y');
  });
});

describe('image and block math act on themselves like the divider', () => {
  const leaves: [string, JSONContent][] = [
    [
      'image',
      { type: 'image', attrs: { src: '/api/files/01a14e70-ad23-7648-9f7d-ccb0f85c5b5c/content' } },
    ],
    ['blockMath', { type: 'blockMath', attrs: { latex: 'x^2' } }],
  ];

  it.each(leaves)('%s: the hit, delete, move and duplicate address the leaf only', (name, leaf) => {
    const e = make({ type: 'doc', content: [p('a'), leaf, p('b')] });
    const from = e.state.doc.child(0).nodeSize;
    expect(blockAtHit(e.state.doc, { pos: from, inside: from })?.node.type.name).toBe(name);
    const at = (): NonNullable<ReturnType<typeof blockStartingAt>> => {
      const block = blockStartingAt(e.state.doc, from);
      if (block === undefined) throw new Error('block');
      return block;
    };
    expect(at().node.type.name).toBe(name);
    runBlockOp(e, 'duplicate', at());
    expect(e.getJSON().content.map((n) => n.type)).toEqual(['paragraph', name, name, 'paragraph']);
    runBlockOp(e, 'delete', at());
    expect(e.getJSON().content.map((n) => n.type)).toEqual(['paragraph', name, 'paragraph']);
    runBlockOp(e, 'down', at());
    // 끝 블록이 문단이 아니면 StarterKit이 빈 문단을 하나 붙인다(TrailingNode).
    expect(
      e
        .getJSON()
        .content.map((n) => n.type)
        .slice(0, 3),
    ).toEqual(['paragraph', 'paragraph', name]);
    expect(texts(e).slice(0, 2)).toEqual(['a', 'b']);
  });
});

describe('the pinned block follows edits by others while its menu is open', () => {
  it('moves past text inserted earlier and is dropped when its block is deleted', () => {
    const e = make({
      type: 'doc',
      content: [p('a'), { type: 'horizontalRule' }, { type: 'blockMath', attrs: { latex: 'y' } }],
    });
    const mathFrom = e.state.doc.child(0).nodeSize + e.state.doc.child(1).nodeSize;
    // 앞 문단에 글자를 넣는다(다른 사람의 편집도 같은 트랜잭션 위치 대응을 거친다).
    const insert = e.state.tr.insertText('앞에 쓴 글', 1);
    e.view.dispatch(insert);
    const moved = mapBlockStart(e.state.doc, insert.mapping, mathFrom);
    expect(moved).toBe(mathFrom + '앞에 쓴 글'.length);
    if (moved === undefined) throw new Error('moved');
    expect(e.state.doc.nodeAt(moved)?.type.name).toBe('blockMath');
    // 앞 블록(구분선)을 지워도 따라간다.
    const hrFrom = e.state.doc.child(0).nodeSize;
    const dropHr = e.state.tr.delete(hrFrom, hrFrom + 1);
    e.view.dispatch(dropHr);
    const after = mapBlockStart(e.state.doc, dropHr.mapping, moved);
    if (after === undefined) throw new Error('after');
    expect(e.state.doc.nodeAt(after)?.type.name).toBe('blockMath');
    // 대상 블록이 지워지면 메뉴를 닫는다(엉뚱한 블록으로 옮기지 않는다).
    const dropMath = e.state.tr.delete(after, after + 1);
    e.view.dispatch(dropMath);
    expect(mapBlockStart(e.state.doc, dropMath.mapping, after)).toBeUndefined();
  });
});

describe('remote edits replace whole top-level blocks', () => {
  it('closes the menu when someone edits inside the same top-level block, keeps it for others', () => {
    const e = make({
      type: 'doc',
      content: [{ type: 'taskList', content: [task('x'), task('y')] }, p('after')],
    });
    const yFrom = blockAt(e.state.doc, posOf(e, 'y'))?.from;
    const afterFrom = e.state.doc.child(0).nodeSize;
    if (yFrom === undefined) throw new Error('y');
    // 실시간 편집(y-tiptap)은 바뀐 최상위 블록을 통째로 바꿔 넣는다: 같은 목록의 x 항목을 고친 경우.
    const list = e.state.doc.child(0);
    const edited = e.schema.nodeFromJSON({
      type: 'taskList',
      content: [task('x 고침'), task('y')],
    });
    const tr = e.state.tr.replaceWith(0, list.nodeSize, edited);
    e.view.dispatch(tr);
    // 같은 최상위 블록 안이면 고정 위치를 지워진 것으로 보고 메뉴를 닫는다(엉뚱한 항목을 조작하지 않는다).
    expect(mapBlockStart(e.state.doc, tr.mapping, yFrom)).toBeUndefined();
    // 다른 최상위 블록은 바뀐 크기만큼 옮겨 그대로 따라간다.
    const moved = mapBlockStart(e.state.doc, tr.mapping, afterFrom);
    if (moved === undefined) throw new Error('moved');
    expect(e.state.doc.nodeAt(moved)?.textContent).toBe('after');
  });
});

describe('several transactions in one moment', () => {
  it('maps the pinned block through each of them in order', () => {
    const e = make({ type: 'doc', content: [p('a'), { type: 'horizontalRule' }, p('b')] });
    const hrFrom = e.state.doc.child(0).nodeSize;
    const first = e.state.tr.insertText('12', 1);
    e.view.dispatch(first);
    const second = e.state.tr.insertText('3', 1);
    e.view.dispatch(second);
    const moved = mapBlockStartThrough([first, second], hrFrom);
    expect(moved).toBe(hrFrom + 3);
    if (moved === undefined) throw new Error('moved');
    const drop = e.state.tr.delete(moved, moved + 1);
    e.view.dispatch(drop);
    const later = e.state.tr.insertText('4', 1);
    e.view.dispatch(later);
    expect(mapBlockStartThrough([drop, later], moved)).toBeUndefined();
  });
});
