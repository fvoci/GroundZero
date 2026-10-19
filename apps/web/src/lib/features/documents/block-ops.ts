// 블록 단위 조작(옮기기·복제·지우기·아래에 추가). 모두 한 번의 ProseMirror 트랜잭션이라 되돌리기·실시간
// 공동 편집(Yjs)에 그대로 실린다. 문서를 통째로 바꾸거나 따로 상태를 들고 있지 않는다.
// 블록: 목록·체크리스트 안이면 가장 안쪽 항목, 아니면 문서 바로 아래 블록.
import type { Editor } from '@tiptap/core';
import { Extension } from '@tiptap/core';
import type { Node as PmNode } from '@tiptap/pm/model';
import type { EditorState, Transaction } from '@tiptap/pm/state';
import { NodeSelection, TextSelection } from '@tiptap/pm/state';

const ITEM_TYPES = new Set(['listItem', 'taskItem']);

export interface BlockRange {
  /** 블록 시작 위치(블록 노드 바로 앞). */
  from: number;
  /** 블록 끝 위치(블록 노드 바로 뒤). */
  to: number;
  node: PmNode;
  parent: PmNode;
  /** 부모 안에서의 순서. */
  index: number;
  /** 부모 노드의 시작 위치(문서면 -1). */
  parentFrom: number;
}

/** `pos`가 들어 있는 블록. 문서 밖이거나 빈 문서면 undefined. */
export function blockAt(doc: PmNode, pos: number): BlockRange | undefined {
  if (doc.childCount === 0) return undefined;
  const $pos = doc.resolve(Math.max(0, Math.min(pos, doc.content.size)));
  let depth = 0;
  for (let d = $pos.depth; d >= 1; d -= 1) {
    if (ITEM_TYPES.has($pos.node(d).type.name)) {
      depth = d;
      break;
    }
  }
  if (depth === 0) {
    if ($pos.depth >= 1) depth = 1;
    else {
      // 문서 바로 아래(블록 사이)를 가리키면 그 뒤 블록, 끝이면 마지막 블록.
      const index = Math.min($pos.index(0), doc.childCount - 1);
      let from = 0;
      for (let i = 0; i < index; i += 1) from += doc.child(i).nodeSize;
      const node = doc.child(index);
      return { from, to: from + node.nodeSize, node, parent: doc, index, parentFrom: -1 };
    }
  }
  const from = $pos.before(depth);
  const node = $pos.node(depth);
  return {
    from,
    to: from + node.nodeSize,
    node,
    parent: $pos.node(depth - 1),
    index: $pos.index(depth - 1),
    parentFrom: depth - 1 === 0 ? -1 : $pos.before(depth - 1),
  };
}

/** `from` 바로 뒤에서 시작하는 노드의 범위(목록 하나를 통째로 다룰 때). */
function nodeRangeAt(doc: PmNode, from: number): BlockRange {
  const $from = doc.resolve(from);
  const node = doc.nodeAt(from) ?? $from.parent;
  return {
    from,
    to: from + node.nodeSize,
    node,
    parent: $from.parent,
    index: $from.index(),
    parentFrom: $from.depth === 0 ? -1 : $from.before(),
  };
}

/** 선택이 들어 있던 블록 안의 상대 위치를 옮긴 블록에서 되살린다. */
function keepSelection(
  tr: Transaction,
  state: EditorState,
  block: BlockRange,
  newFrom: number,
): void {
  const offset = Math.max(1, Math.min(state.selection.from - block.from, block.node.nodeSize - 1));
  const pos = Math.min(newFrom + offset, tr.doc.content.size);
  tr.setSelection(TextSelection.near(tr.doc.resolve(pos)));
}

/** 같은 부모 안에서 한 칸 위(-1)나 아래(+1)로. 끝이면 아무것도 하지 않는다. */
export function moveBlock(
  state: EditorState,
  block: BlockRange,
  dir: -1 | 1,
): Transaction | undefined {
  const sibling = dir === -1 ? block.index - 1 : block.index + 1;
  if (sibling < 0 || sibling >= block.parent.childCount) return undefined;
  const other = block.parent.child(sibling);
  const tr = state.tr;
  tr.delete(block.from, block.to);
  const target = dir === -1 ? block.from - other.nodeSize : block.from + other.nodeSize;
  tr.insert(target, block.node);
  keepSelection(tr, state, block, target);
  return tr.scrollIntoView();
}

/**
 * 바로 아래에 같은 블록을 하나 더 만든다. 편집기 안 체크리스트 항목은 문서 내용일 뿐 id가 없고(업무·편집기 밖
 * 체크리스트의 행과 연결되지 않는다), 참조(@)는 원본을 가리키는 링크이므로 복제해도 원본 업무·항목의 상태를
 * 바꾸거나 공유하지 않는다. 체크 상태는 보이는 그대로 복사한다.
 */
export function duplicateBlock(state: EditorState, block: BlockRange): Transaction {
  const tr = state.tr.insert(block.to, block.node);
  keepSelection(tr, state, block, block.to);
  return tr.scrollIntoView();
}

/** 블록을 지운다. 목록의 마지막 항목이면 목록째, 문서의 마지막 블록이면 빈 문단으로 바꾼다. */
export function deleteBlock(state: EditorState, block: BlockRange): Transaction {
  const tr = state.tr;
  const listOnlyItem = ITEM_TYPES.has(block.node.type.name) && block.parent.childCount === 1;
  if (listOnlyItem && block.parentFrom >= 0)
    return deleteBlock(state, nodeRangeAt(state.doc, block.parentFrom));
  if (block.parent === state.doc && state.doc.childCount === 1) {
    const paragraph = state.schema.nodes.paragraph;
    if (paragraph !== undefined) tr.replaceWith(block.from, block.to, paragraph.create());
  } else {
    tr.delete(block.from, block.to);
  }
  const pos = Math.min(block.from + 1, tr.doc.content.size);
  tr.setSelection(TextSelection.near(tr.doc.resolve(pos), -1));
  return tr.scrollIntoView();
}

/** 바로 아래에 빈 블록(목록 안이면 같은 종류의 빈 항목, 체크 항목은 미완료)을 넣고 커서를 옮긴다. */
export function insertBlockBelow(state: EditorState, block: BlockRange): Transaction | undefined {
  const { schema } = state;
  const paragraph = schema.nodes.paragraph;
  if (paragraph === undefined) return undefined;
  const type = block.node.type;
  const node = ITEM_TYPES.has(type.name)
    ? type.create(type.name === 'taskItem' ? { checked: false } : null, paragraph.create())
    : paragraph.create();
  const tr = state.tr.insert(block.to, node);
  tr.setSelection(TextSelection.near(tr.doc.resolve(block.to + 1)));
  return tr.scrollIntoView();
}

/** 끌어 옮기기를 시작할 때 쓸 노드 선택(ProseMirror 기본 끌어 놓기가 옮긴다). */
export function selectBlock(state: EditorState, block: BlockRange): Transaction {
  return state.tr.setSelection(NodeSelection.create(state.doc, block.from));
}

/**
 * 블록 시작 위치(`from`, 블록 노드 바로 앞)에 있는 블록. 손잡이·메뉴는 이 위치를 들고 다닌다. 구분선·이미지·
 * 블록 수식처럼 안쪽이 없는 블록도 그 블록을 가리킨다(시작 + 1은 다음 블록이 되므로 쓰지 않는다).
 */
export function blockStartingAt(doc: PmNode, from: number): BlockRange | undefined {
  if (from < 0 || from >= doc.content.size) return undefined;
  const node = doc.nodeAt(from);
  if (!node?.isBlock) return undefined;
  return nodeRangeAt(doc, from);
}

/**
 * 문서가 바뀐 뒤(다른 사람의 편집 등) 블록 시작 위치를 옮긴다. 그 블록이 지워졌거나 통째로 바뀌었으면
 * undefined(메뉴를 닫는다). 앞에 끼워 넣은 내용은 위치를 뒤로 민다.
 */
export function mapBlockStart(
  doc: PmNode,
  mapping: { mapResult: (pos: number, assoc?: number) => { pos: number; deleted: boolean } },
  from: number,
): number | undefined {
  const mapped = mapping.mapResult(from, 1);
  if (mapped.deleted || blockStartingAt(doc, mapped.pos) === undefined) return undefined;
  return mapped.pos;
}

/** 한 순간에 온 트랜잭션들(덧붙인 것 포함)을 차례로 거쳐 블록 시작 위치를 옮긴다. 도중에 지워지면 undefined. */
export function mapBlockStartThrough(
  transactions: readonly Transaction[],
  from: number,
): number | undefined {
  let pos: number | undefined = from;
  for (const tr of transactions) {
    if (pos === undefined) return undefined;
    pos = mapBlockStart(tr.doc, tr.mapping, pos);
  }
  return pos;
}

/** 화면 좌표의 `posAtCoords` 결과(`inside`: 그 위치를 품은 노드의 시작)로 블록을 찾는다. */
export function blockAtHit(
  doc: PmNode,
  hit: { pos: number; inside: number },
): BlockRange | undefined {
  if (hit.inside >= 0) {
    const node = doc.nodeAt(hit.inside);
    // 안쪽이 없는 블록(구분선·이미지·블록 수식)은 그 자체가 블록이다.
    if (node?.isBlock === true && node.isLeaf) return blockStartingAt(doc, hit.inside);
    return blockAt(doc, hit.inside + 1);
  }
  return blockAt(doc, hit.pos);
}

/** 블록 손잡이 줄의 높이(px, BlockControls의 h-7). */
export const HANDLE_ROW = 28;

/**
 * 손잡이 줄을 블록 위쪽에서 얼마나 옮길지(px). 손잡이보다 낮은 블록(1px 구분선 등)은 손잡이 가운데를 블록
 * 가운데에 맞춘다. 위쪽에 맞추면 손잡이가 블록 아래 여백에 걸려, 손잡이로 가는 길에 다음 블록을 가리키게 된다.
 */
export function handleOffset(blockHeight: number): number {
  return blockHeight < HANDLE_ROW ? (blockHeight - HANDLE_ROW) / 2 : 0;
}

type Op = 'up' | 'down' | 'duplicate' | 'delete' | 'below';

/**
 * 블록에 조작을 적용한다. `at`이 없으면 선택이 있는 블록, 숫자면 그 위치를 품은 블록(블록 안 위치),
 * BlockRange면 그 블록. 바꿨으면 true.
 */
export function runBlockOp(editor: Editor, op: Op, at?: number | BlockRange): boolean {
  if (!editor.isEditable) return false;
  const { state, view } = editor;
  const block = typeof at === 'object' ? at : blockAt(state.doc, at ?? state.selection.from);
  if (block === undefined) return false;
  let tr: Transaction | undefined;
  switch (op) {
    case 'up':
      tr = moveBlock(state, block, -1);
      break;
    case 'down':
      tr = moveBlock(state, block, 1);
      break;
    case 'duplicate':
      tr = duplicateBlock(state, block);
      break;
    case 'delete':
      tr = deleteBlock(state, block);
      break;
    case 'below':
      tr = insertBlockBelow(state, block);
      break;
  }
  if (tr === undefined) return false;
  view.dispatch(tr);
  view.focus();
  return true;
}

/** 키보드: Alt+Shift+↑/↓ 옮기기, Mod+Shift+D 복제. 블록 메뉴 열기는 편집기 화면이 맡는다(`onMenu`). */
export const BlockKeys = Extension.create<{ onMenu: (pos: number) => void }>({
  name: 'blockKeys',
  addOptions() {
    return { onMenu: () => undefined };
  },
  addKeyboardShortcuts() {
    return {
      'Alt-Shift-ArrowUp': () => runBlockOp(this.editor, 'up'),
      'Alt-Shift-ArrowDown': () => runBlockOp(this.editor, 'down'),
      'Mod-Shift-d': () => runBlockOp(this.editor, 'duplicate'),
      'Mod-/': () => {
        if (!this.editor.isEditable) return false;
        this.options.onMenu(this.editor.state.selection.from);
        return true;
      },
    };
  },
});

/** 블록 안 위치(`pos`, 블록 시작 + 1 이상)에서 가장 가까운 글자 자리로 커서를 옮긴다(유형 바꾸기 전에). */
export function focusBlock(editor: Editor, pos: number): void {
  const { state, view } = editor;
  const at = Math.min(pos, state.doc.content.size);
  view.dispatch(state.tr.setSelection(TextSelection.near(state.doc.resolve(at))));
}
