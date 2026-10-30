// 미저장 초안 보호 (D-27). 브라우저 로컬 저장소에 사용자·문서별로 마지막 편집 내용을 둔다(같은 브라우저를 쓰는 다른
// 사용자에게 보이지 않게 키에 사용자 id를 넣는다. 로그인 전이면 두지 않는다).
// 개인 편의용이며 서버 저장을 대신하지 않는다. 저장소를 쓸 수 없는 환경(사생활 보호 모드 등)에서는 조용히 생략한다.
import type { DocNode } from '$lib/api/client';
import { parseDocNode } from '$lib/api/client';
import { readDraftAt, writeDraftAt } from '$lib/features/common/draft-storage';
import { me } from '$lib/me.svelte';
import { scopedKey } from '$lib/storage';

export interface Draft {
  baseRevision: number;
  title: string;
  body: DocNode;
  savedAt: string;
}

// 이 탭의 id. 같은 사람이 같은 문서를 여러 탭에서 열어도 서로의 초안을 덮거나 지우지 않게, 지금 편집의 초안은
// 탭마다 따로 둔다. 새로고침해도 같은 탭이면 같은 id(sessionStorage). 쓸 수 없으면 이 페이지 동안만 쓰는 id.
let memoryTab: string | undefined;
export function tabId(): string {
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

/**
 * 이 사용자·문서의 초안 키 앞부분. 로그인 전이면 없다. 조직은 넣지 않는다(폼 초안 `form-draft.ts`와 같은 규칙): 개인
 * 노트는 조직과 무관하고, 되살릴 때의 조직 문맥이 남길 때와 다를 수 있다(⑱·D-27).
 */
function base(documentId: string): string | undefined {
  return me.userId === '' ? undefined : scopedKey(`draft:${documentId}`, me.userId, undefined);
}

/** 이 탭의 지금 편집의 초안 키. */
function key(documentId: string): string | undefined {
  const b = base(documentId);
  return b === undefined ? undefined : `${b}:tab:${tabId()}`;
}

/**
 * 고르게 할 지난 초안들. 초안마다 키 하나(`<앞부분>:offer:<순서>.<id>`)에 두고 더한 순서로 고르게 한다. 다시 열 때 찾은
 * 초안을 사용자가 고르는 동안 새 편집은 탭의 키에 따로 남고, 그것도 다음에 이 줄에 붙는다. 어느 것도 덮거나 버리지
 * 않는다. 한 키에 목록을 두면 두 탭이 거의 동시에 읽고 고쳐 쓸 때 한쪽 것이 사라진다(localStorage는 탭 사이에
 * 원자적이지 않다).
 */
function offerPrefix(documentId: string): string | undefined {
  const b = base(documentId);
  return b === undefined ? undefined : `${b}:offer:`;
}

/** 같은 밀리초에 더한 것끼리의 순서. */
let pushed = 0;

/**
 * 초안 하나를 줄 끝에 더한다(키가 더한 순서대로 정렬된다). 쓰기에 성공했는지.
 */
function pushOffer(documentId: string, draft: Draft): boolean {
  const prefix = offerPrefix(documentId);
  if (prefix === undefined) return false;
  pushed += 1;
  const order = `${String(Date.now()).padStart(15, '0')}.${String(pushed).padStart(9, '0')}`;
  return writeDraftAt(`${prefix}${order}.${crypto.randomUUID()}`, draft);
}

/** 줄의 초안과 그 키(더한 순). */
function offerEntries(documentId: string): { key: string; draft: Draft }[] {
  const prefix = offerPrefix(documentId);
  if (prefix === undefined) return [];
  const keys: string[] = [];
  try {
    for (let i = 0; i < window.localStorage.length; i += 1) {
      const k = window.localStorage.key(i);
      if (k?.startsWith(prefix) === true) keys.push(k);
    }
  } catch {
    return [];
  }
  return keys
    .map((k) => ({ key: k, draft: readDraftAt(k, parseDraft) }))
    .filter((e): e is { key: string; draft: Draft } => e.draft !== undefined)
    .sort((a, b) => (a.key < b.key ? -1 : a.key > b.key ? 1 : 0));
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
 * 페이지를 열 때 이 탭 id의 잠금을 바로 잡는다. 탭 복제는 sessionStorage까지 복사하므로, 이미 살아 있는 탭이 이
 * id를 쥐고 있으면 이 탭은 새 id를 쓴다(같은 id면 두 탭이 같은 초안 키를 덮어쓴다). 초안 키는 로그인 정보를 받은
 * 뒤에야 생기므로 그 전에 끝난다.
 */
function claimTab(): void {
  const locks = tabLocks();
  if (locks === undefined || typeof window === 'undefined') return;
  holdingLock = true;
  void locks
    .request(`fvoci:tab:${tabId()}`, { ifAvailable: true }, (lock) => {
      if (lock !== null) return new Promise<never>(() => undefined);
      const fresh = crypto.randomUUID();
      try {
        window.sessionStorage.setItem('fvoci:tab', fresh);
      } catch {
        memoryTab = fresh;
      }
      void locks
        .request(`fvoci:tab:${fresh}`, () => new Promise<never>(() => undefined))
        .catch(() => undefined);
      return undefined;
    })
    .catch(() => {
      holdingLock = false;
    });
}
claimTab();

/**
 * 닫힌 탭이 남긴 이 문서의 초안을 고르게 할 줄로 넘겨받는다. 살아 있는 탭의 것은 건드리지
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
      const tab = k.startsWith(`${b}:tab:`) ? k.slice(`${b}:tab:`.length) : undefined;
      if (tab !== undefined && !held.has(`fvoci:tab:${tab}`)) candidates.push(k);
    }
  } catch {
    return 0;
  }
  let adopted = 0;
  for (const k of candidates) {
    const d = readDraftAt(k, parseDraft);
    if (d !== undefined && !pushOffer(documentId, d)) continue;
    writeDraftAt(k, undefined);
    if (d !== undefined) adopted += 1;
  }
  return adopted;
}

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

export function loadDraft(documentId: string): Draft | undefined {
  return readDraftAt(key(documentId), parseDraft);
}

export function storeDraft(documentId: string, draft: Draft): boolean {
  holdTabLock();
  return writeDraftAt(key(documentId), draft);
}

export function clearDraft(documentId: string): void {
  writeDraftAt(key(documentId), undefined);
}

/** 고르게 할 지난 초안들(먼저 남은 것부터). */
export function loadOffers(documentId: string): Draft[] {
  return offerEntries(documentId).map((e) => e.draft);
}

/** 고르게 할 줄에서 이 초안을 뺀다(보존했거나 버렸거나 이미 반영됐다). 다른 초안의 키는 건드리지 않는다. */
export function removeOffer(documentId: string, done: Draft): void {
  for (const { key: k, draft: d } of offerEntries(documentId)) {
    if (d.savedAt === done.savedAt && sameContent(d, done)) writeDraftAt(k, undefined);
  }
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
  const main = loadDraft(documentId);
  if (main !== undefined && pushOffer(documentId, main)) clearDraft(documentId);
  return loadOffers(documentId);
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
