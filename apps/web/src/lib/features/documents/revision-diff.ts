// 리비전 비교 (D-27). 본문 JSON을 블록마다 한 줄의 읽을 수 있는 글자로 펴고(제목은 #, 목록은 -),
// 줄 단위 차이를 jsdiff로 계산한다. 참조 노드는 plainText와 같은 자리표시(@[종류:id])로 나온다.
import { diffArrays } from 'diff';

import { plainText } from './drafts';

export interface DiffLine {
  kind: 'same' | 'added' | 'removed';
  text: string;
}

function isNode(value: unknown): value is { type?: unknown; attrs?: unknown; content?: unknown } {
  return typeof value === 'object' && value !== null;
}

function children(node: { content?: unknown }): unknown[] {
  return Array.isArray(node.content) ? node.content : [];
}

function checked(attrs: unknown): boolean {
  return (
    typeof attrs === 'object' && attrs !== null && 'checked' in attrs && attrs.checked === true
  );
}

function headingLevel(attrs: unknown): number {
  if (
    typeof attrs === 'object' &&
    attrs !== null &&
    'level' in attrs &&
    typeof attrs.level === 'number'
  ) {
    return attrs.level;
  }
  return 1;
}

function linesOf(node: unknown, depth: number, out: string[]): void {
  if (!isNode(node)) return;
  const indent = '  '.repeat(depth);
  switch (node.type) {
    case 'bulletList':
    case 'orderedList':
    case 'taskList': {
      let n = 1;
      for (const item of children(node)) {
        const mark =
          node.type === 'orderedList'
            ? `${String(n)}.`
            : node.type === 'taskList'
              ? `- [${isNode(item) && checked(item.attrs) ? 'x' : ' '}]`
              : '-';
        n += 1;
        if (!isNode(item)) continue;
        const [first, ...rest] = children(item);
        out.push(`${indent}${mark} ${plainText(first)}`);
        for (const child of rest) linesOf(child, depth + 1, out);
      }
      return;
    }
    case 'heading':
      out.push(`${indent}${'#'.repeat(headingLevel(node.attrs))} ${plainText(node)}`);
      return;
    case 'blockquote':
      for (const child of children(node)) out.push(`${indent}> ${plainText(child)}`);
      return;
    case 'codeBlock':
      for (const line of plainText(node).split('\n')) out.push(`${indent}    ${line}`);
      return;
    case 'horizontalRule':
      out.push(`${indent}---`);
      return;
    default: {
      const text = plainText(node);
      if (text.trim() !== '') out.push(`${indent}${text}`);
    }
  }
}

/** 본문을 블록 줄로 편다. */
export function blockLines(body: unknown): string[] {
  const out: string[] = [];
  if (isNode(body)) for (const block of children(body)) linesOf(block, 0, out);
  return out;
}

/** 옛 것과 새 것의 줄 단위 차이. 제목이 바뀌었으면 첫 줄에 나온다. */
export function diffRevisions(
  before: { title: string; body: unknown },
  after: { title: string; body: unknown },
): DiffLine[] {
  const a = [`제목: ${before.title}`, ...blockLines(before.body)];
  const b = [`제목: ${after.title}`, ...blockLines(after.body)];
  const out: DiffLine[] = [];
  for (const part of diffArrays(a, b)) {
    const kind = part.added ? 'added' : part.removed ? 'removed' : 'same';
    for (const text of part.value) out.push({ kind, text });
  }
  return out;
}
