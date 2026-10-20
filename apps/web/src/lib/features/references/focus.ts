// 백링크로 원문 문서를 열 때 그 문서 안에서 대상을 가리키는 참조 자리로 가기 (D-27). 백링크 목록에는 위치가 없으므로
// 대상(종류·id)을 주소의 `#`로 넘기고, 원문 화면이 본문에 그려진 참조 노드(`data-kind`·`data-id`)를 찾는다.
import type { ReferenceKind } from '$lib/api/client';

export interface ReferenceTarget {
  kind: Extract<ReferenceKind, 'document' | 'item' | 'task'>;
  id: string;
}

const PATTERN = /^#ref-(document|item|task)-([0-9a-fA-F-]{36})$/;

export function referenceHash(target: ReferenceTarget): string {
  return `#ref-${target.kind}-${target.id}`;
}

export function parseReferenceHash(hash: string): ReferenceTarget | undefined {
  const match = PATTERN.exec(hash);
  const kind = match?.[1];
  const id = match?.[2];
  if (kind === undefined || id === undefined) return undefined;
  return { kind: kind === 'document' ? 'document' : kind === 'item' ? 'item' : 'task', id };
}

/** 본문에서 그 대상을 가리키는 참조 노드들(본문 순서). */
export function referenceNodes(root: ParentNode, target: ReferenceTarget): HTMLElement[] {
  return [
    ...root.querySelectorAll<HTMLElement>(
      `[data-reference][data-kind="${CSS.escape(target.kind)}"][data-id="${CSS.escape(target.id)}"]`,
    ),
  ];
}
