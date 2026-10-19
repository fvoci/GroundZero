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

// 이 탭의 id. 같은 사람이 같은 문서를 여러 탭에서 열어도 서로의 초안을 덮거나 지우지 않게, 지금 편집의 초안은
// 탭마다 따로 둔다. 새로고침해도 같은 탭이면 같은 id(sessionStorage). 쓸 수 없으면 이 페이지 동안만 쓰는 id.
let memoryTab: string | undefined;
function tabId(): string {
  try {
    let id = window.sessionStorage.getItem('fvoci:tab');
    if (id === null) {
      id = crypto.randomUUID();
      window.sessionStorage.setItem('fvoci:tab', id);
    }
    return id;
  } catch {
    memoryTab ??= crypto.randomUUID();
    return memoryTab;
  }
}

/** 이 사용자·문서의 초안 키 앞부분. 로그인 전이면 없다. */
function base(documentId: string): string | undefined {
  return me.userId === '' ? undefined : `fvoci:draft:${me.userId}:${documentId}`;
}

/** 이 탭의 지금 편집의 초안 키. */
function key(documentId: string): string | undefined {
  const b = base(documentId);
  return b === undefined ? undefined : `${b}:tab:${tabId()}`;
}

/**
 * 고르게 할 지난 초안들의 키(사용자·문서마다 하나, 순서대로 쌓는다). 다시 열 때 찾은 초안을 사용자가 고르는 동안
 * 새 편집은 탭의 키에 따로 남고, 그것도 다음에 이 줄 뒤에 붙는다. 어느 것도 덮거나 버리지 않는다.
 */
function offersKey(documentId: string): string | undefined {
  const b = base(documentId);
  return b === undefined ? undefined : `${b}:offers`;
}

/** 이 탭이 살아 있다는 표시(Web Locks). 다른 탭은 이 잠금이 없을 때만 이 탭의 초안을 넘겨받는다. */
let holdingLock = false;

/** Web Locks(없는 환경도 있다: 오래된 브라우저, 시험 환경). */
function tabLocks(): LockManager | undefined {
  return typeof navigator !== 'undefined' && 'locks' in navigator ? navigator.locks : undefined;
}

function holdTabLock(): void {
  const locks = tabLocks();
  if (holdingLock || locks === undefined) return;
  holdingLock = true;
  void locks
    .request(`fvoci:tab:${tabId()}`, () => new Promise<never>(() => undefined))
    .catch(() => undefined);
}

/**
 * 닫힌 탭이 남긴 이 문서의 초안(과 탭 구분이 없던 키의 것)을 고르게 할 줄로 넘겨받는다. 살아 있는 탭의 것은 건드리지
 * 않는다. 탭이 살아 있는지 알 수 없는 브라우저(Web Locks 없음)에서는 아무것도 넘겨받지 않는다. 넘겨받은 수.
 */
export async function adoptOrphans(documentId: string): Promise<number> {
  const b = base(documentId);
  const locks = tabLocks();
  if (b === undefined || locks === undefined) return 0;
  holdTabLock();
  let held: Set<string>;
  try {
    const state = await locks.query();
    held = new Set((state.held ?? []).map((l) => l.name ?? ''));
  } catch {
    return 0;
  }
  const mine = key(documentId);
  const candidates: string[] = [];
  try {
    for (let i = 0; i < window.localStorage.length; i += 1) {
      const k = window.localStorage.key(i);
      if (k === null || k === mine) continue;
      if (k === b) candidates.push(k);
      const tab = k.startsWith(`${b}:tab:`) ? k.slice(`${b}:tab:`.length) : undefined;
      if (tab !== undefined && !held.has(`fvoci:tab:${tab}`)) candidates.push(k);
    }
  } catch {
    return 0;
  }
  let adopted = 0;
  for (const k of candidates) {
    const d = parseDraft(read(k));
    if (d !== undefined && !storeOffers(documentId, [...loadOffers(documentId), d])) continue;
    write(k, undefined);
    if (d !== undefined) adopted += 1;
  }
  return adopted;
}

/** 사용자 id가 없던 옛 키. 누구의 것인지 알 수 없어 제안하지 않고 지운다. */
const legacyKey = (documentId: string): string => `fvoci:draft:${documentId}`;

function parseDraft(value: unknown): Draft | undefined {
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
  try {
    return { baseRevision, title, body: parseDocNode(body), savedAt };
  } catch {
    return undefined;
  }
}

function read(k: string | undefined): unknown {
  if (k === undefined) return undefined;
  try {
    const raw = window.localStorage.getItem(k);
    if (raw === null) return undefined;
    const value: unknown = JSON.parse(raw);
    return value;
  } catch {
    return undefined;
  }
}

/** 쓰기에 성공했는지. 저장소가 가득 찼거나 쓸 수 없으면 거짓(옮기는 쪽은 그때 원본을 지우지 않는다). */
function write(k: string | undefined, value: unknown): boolean {
  if (k === undefined) return false;
  try {
    if (value === undefined) window.localStorage.removeItem(k);
    else window.localStorage.setItem(k, JSON.stringify(value));
    return true;
  } catch {
    // 저장소를 쓸 수 없으면 초안 보관만 생략한다. 서버 저장 흐름은 영향받지 않는다.
    return false;
  }
}

export function loadDraft(documentId: string): Draft | undefined {
  try {
    window.localStorage.removeItem(legacyKey(documentId));
  } catch {
    // 위와 같다.
  }
  return parseDraft(read(key(documentId)));
}

export function storeDraft(documentId: string, draft: Draft): boolean {
  holdTabLock();
  return write(key(documentId), draft);
}

export function clearDraft(documentId: string): void {
  write(key(documentId), undefined);
}

/** 고르게 할 지난 초안들(먼저 남은 것부터). */
export function loadOffers(documentId: string): Draft[] {
  const value = read(offersKey(documentId));
  const list: unknown[] = Array.isArray(value) ? value : value === undefined ? [] : [value];
  return list.map(parseDraft).filter((d): d is Draft => d !== undefined);
}

export function storeOffers(documentId: string, drafts: readonly Draft[]): boolean {
  return write(offersKey(documentId), drafts.length === 0 ? undefined : drafts);
}

/** 고르게 할 줄에서 이 초안 하나를 뺀다(보존했거나 버렸다). */
export function removeOffer(documentId: string, done: Draft): void {
  storeOffers(
    documentId,
    loadOffers(documentId).filter((d) => d.savedAt !== done.savedAt || !sameContent(d, done)),
  );
}

/** 두 초안(또는 초안과 지금 내용)이 같은지: 제목과 본문 모두. */
export function sameContent(
  a: { title: string; body: unknown },
  b: { title: string; body: unknown },
): boolean {
  return (
    canonicalJson({ title: a.title.trim(), body: a.body }) ===
    canonicalJson({ title: b.title.trim(), body: b.body })
  );
}

/** 지금 내용(제목·본문)과 같아 이미 반영된 초안을 뺀 줄. */
export function withoutApplied(
  drafts: readonly Draft[],
  current: { title: string; body: unknown },
): Draft[] {
  return drafts.filter((d) => !sameContent(d, current));
}

/**
 * 지금 편집의 초안을 고르게 할 줄 뒤로 옮긴다(덮이지 않게). 줄에 쓰지 못했으면(저장소 가득 참) 지금 초안을 지우지
 * 않고 그대로 둔다. 옮긴 뒤의 줄을 돌려준다.
 */
export function queueDraft(documentId: string): Draft[] {
  const offers = loadOffers(documentId);
  const main = loadDraft(documentId);
  if (main === undefined) return offers;
  const next = [...offers, main];
  if (!storeOffers(documentId, next)) return offers;
  clearDraft(documentId);
  return next;
}

/**
 * 편집을 시작할 때: 지금 편집의 초안을 줄로 옮긴다. 옮기지 못했으면(저장소 가득 참) 그 초안을 `reserved`로 돌려준다.
 * 부르는 쪽은 그것을 사용자에게 고르게 하고, 고를 때까지 그 키에 새 편집을 쓰거나 지우지 않는다.
 */
export function takeOrReserve(documentId: string): { reserved: Draft | undefined } {
  queueDraft(documentId);
  return { reserved: loadDraft(documentId) };
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
