// 미저장 초안 보호 (D-27). 브라우저 로컬 저장소에 문서별로 마지막 편집 내용을 둔다.
// 개인 편의용이며 서버 저장을 대신하지 않는다. 저장소를 쓸 수 없는 환경(사생활 보호 모드 등)에서는 조용히 생략한다.
import type { DocNode } from '$lib/api/client';
import { parseDocNode } from '$lib/api/client';

export interface Draft {
  baseRevision: number;
  title: string;
  body: DocNode;
  savedAt: string;
}

const key = (documentId: string): string => `fvoci:draft:${documentId}`;

export function loadDraft(documentId: string): Draft | undefined {
  try {
    const raw = window.localStorage.getItem(key(documentId));
    if (raw === null) return undefined;
    const value: unknown = JSON.parse(raw);
    if (typeof value !== 'object' || value === null) return undefined;
    if (
      !('baseRevision' in value) ||
      !('title' in value) ||
      !('body' in value) ||
      !('savedAt' in value)
    ) {
      return undefined;
    }
    const { baseRevision, title, body, savedAt } = value;
    if (
      typeof baseRevision !== 'number' ||
      typeof title !== 'string' ||
      typeof savedAt !== 'string'
    ) {
      return undefined;
    }
    return { baseRevision, title, body: parseDocNode(body), savedAt };
  } catch {
    return undefined;
  }
}

export function storeDraft(documentId: string, draft: Draft): void {
  try {
    window.localStorage.setItem(key(documentId), JSON.stringify(draft));
  } catch {
    // 저장소를 쓸 수 없으면 초안 보관만 생략한다. 서버 저장 흐름은 영향받지 않는다.
  }
}

export function clearDraft(documentId: string): void {
  try {
    window.localStorage.removeItem(key(documentId));
  } catch {
    // 위와 같다.
  }
}

/** 본문에서 사람이 읽는 텍스트를 뽑는다. 충돌 화면의 비교용. */
export function plainText(node: unknown): string {
  if (typeof node !== 'object' || node === null) return '';
  const parts: string[] = [];
  if ('text' in node && typeof node.text === 'string') parts.push(node.text);
  if ('content' in node && Array.isArray(node.content)) {
    const block = 'type' in node && node.type !== 'text' && node.type !== 'doc';
    const inner = node.content.map((child: unknown) => plainText(child)).join(block ? '' : '\n');
    parts.push(inner);
  }
  return parts.join('');
}

/** 객체 키를 정렬한 JSON. 키 순서만 다른 두 본문을 같게 본다. */
export function canonicalJson(value: unknown): string {
  return JSON.stringify(value, (_key, v: unknown) => {
    if (typeof v !== 'object' || v === null || Array.isArray(v)) return v;
    const entries = Object.entries(v).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
    return Object.fromEntries(entries);
  });
}
