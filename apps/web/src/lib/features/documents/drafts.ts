// 미저장 초안 보호 (D-27). 브라우저 로컬 저장소에 사용자·문서별로 마지막 편집 내용을 둔다(같은 브라우저를 쓰는 다른
// 사용자에게 보이지 않게 키에 사용자 id를 넣는다. 로그인 전이면 두지 않는다).
// 개인 편의용이며 서버 저장을 대신하지 않는다. 저장소를 쓸 수 없는 환경(사생활 보호 모드 등)에서는 조용히 생략한다.
import type { DocNode } from '$lib/api/client';
import { parseDocNode } from '$lib/api/client';
import { me } from '$lib/me.svelte';

export interface Draft {
  baseRevision: number;
  title: string;
  body: DocNode;
  savedAt: string;
}

/**
 * 초안 칸. `offer`: 다시 열 때 찾아 사용자에게 고르게 하는 중인 지난 초안(그동안 새 편집은 기본 칸에 남는다).
 */
export type DraftSlot = 'main' | 'offer';

/** 이 사용자의 초안 키. 로그인 전이면 없다. */
function key(documentId: string, slot: DraftSlot = 'main'): string | undefined {
  if (me.userId === '') return undefined;
  const base = `fvoci:draft:${me.userId}:${documentId}`;
  return slot === 'main' ? base : `${base}:${slot}`;
}

/** 사용자 id가 없던 옛 키. 누구의 것인지 알 수 없어 제안하지 않고 지운다. */
const legacyKey = (documentId: string): string => `fvoci:draft:${documentId}`;

export function loadDraft(documentId: string, slot: DraftSlot = 'main'): Draft | undefined {
  const k = key(documentId, slot);
  try {
    window.localStorage.removeItem(legacyKey(documentId));
    if (k === undefined) return undefined;
    const raw = window.localStorage.getItem(k);
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

export function storeDraft(documentId: string, draft: Draft, slot: DraftSlot = 'main'): void {
  const k = key(documentId, slot);
  if (k === undefined) return;
  try {
    window.localStorage.setItem(k, JSON.stringify(draft));
  } catch {
    // 저장소를 쓸 수 없으면 초안 보관만 생략한다. 서버 저장 흐름은 영향받지 않는다.
  }
}

export function clearDraft(documentId: string, slot: DraftSlot = 'main'): void {
  const k = key(documentId, slot);
  if (k === undefined) return;
  try {
    window.localStorage.removeItem(k);
  } catch {
    // 위와 같다.
  }
}

/** 본문에서 사람이 읽는 텍스트를 뽑는다. 충돌 화면의 비교용. */
export function plainText(node: unknown): string {
  if (typeof node !== 'object' || node === null) return '';
  const parts: string[] = [];
  if ('text' in node && typeof node.text === 'string') parts.push(node.text);
  // 참조 노드는 글자가 없다. 참조만 다른 두 본문이 같아 보이지 않게 종류·id 자리표시를 넣는다.
  if ('type' in node && node.type === 'reference' && 'attrs' in node) {
    const attrs = node.attrs;
    if (typeof attrs === 'object' && attrs !== null && 'kind' in attrs && 'id' in attrs) {
      parts.push(`@[${String(attrs.kind)}:${String(attrs.id)}]`);
    }
  }
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
