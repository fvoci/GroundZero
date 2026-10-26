// 폼 입력의 브라우저 보존(D-96·D-128). 문서 초안(D-27, documents/drafts.ts)과 같은 규칙이다: 사용자별 키라 같은 브라우저의
// 다른 계정에 보이지 않고, 탭별 키라 같은 사람의 두 탭이 서로 덮지 않는다. 서버 저장을 대신하지 않는다(되살린 뒤 사용자가
// 직접 저장·만들기를 눌러야 보낸다). 보존 기간도 문서 초안과 같다: 되살리거나 버리거나 서버 내용과 같아질 때까지(따로 만료
// 없음). 저장소를 못 쓰는 환경(비공개 창 등)은 조용히 생략한다.
import { tabId } from '$lib/features/documents/drafts';
import { me } from '$lib/me.svelte';
import { scopedKey } from '$lib/storage';

/** 저장소에 남은 입력 하나와 그 키. */
export interface StoredFormDraft<T> {
  key: string;
  value: T;
}

/**
 * 이 사용자의 이 이름의 키 앞부분. 로그인 전이면 없다. 조직은 넣지 않는다(업무·항목 id는 조직을 넘어 유일하고, 되살릴 때의
 * 조직 문맥이 남길 때와 다를 수 있다).
 */
function base(name: string, userId = me.userId): string | undefined {
  return userId === '' ? undefined : scopedKey(`form-draft:${name}`, userId, undefined);
}

/** 이 탭의 지금 입력의 키. */
export function formDraftTabKey(name: string, userId = me.userId): string | undefined {
  const b = base(name, userId);
  return b === undefined ? undefined : `${b}:tab:${tabId()}`;
}

function read(key: string): unknown {
  try {
    const raw = window.localStorage.getItem(key);
    if (raw === null) return undefined;
    const value: unknown = JSON.parse(raw);
    return value;
  } catch {
    return undefined;
  }
}

export function writeFormDraftAt(key: string | undefined, value: unknown): boolean {
  if (key === undefined) return false;
  try {
    if (value === undefined) window.localStorage.removeItem(key);
    else window.localStorage.setItem(key, JSON.stringify(value));
    return true;
  } catch {
    // 저장소를 쓸 수 없으면 보존만 생략한다. 서버 저장 흐름은 영향받지 않는다.
    return false;
  }
}

/** 이 탭의 지금 입력을 남긴다. 남겼는지. */
export function storeFormDraft(
  name: string,
  value: Record<string, unknown> & { savedAt: string },
): boolean {
  return writeFormDraftAt(formDraftTabKey(name), value);
}

/** 이 탭의 지금 입력을 지운다(다른 탭·보류한 것은 두고). */
export function clearFormDraft(name: string): void {
  writeFormDraftAt(formDraftTabKey(name), undefined);
}

/**
 * 이 사용자의 `name`(과 그 아래 이름들)의 입력 전부(이 탭·다른 탭·보류). 가장 새로 남긴 것부터. 읽지 못하는 값은 뺀다.
 */
export function listFormDrafts<T extends { savedAt: string }>(
  name: string,
  parse: (value: unknown) => T | undefined,
  userId = me.userId,
): StoredFormDraft<T>[] {
  const b = base(name, userId);
  if (b === undefined) return [];
  const prefix = `${b}:`;
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
    .map((key) => ({ key, value: parse(read(key)) }))
    .filter((e): e is StoredFormDraft<T> => e.value !== undefined)
    .sort((a, b) =>
      a.value.savedAt < b.value.savedAt ? 1 : a.value.savedAt > b.value.savedAt ? -1 : 0,
    );
}

/** 남긴 입력 하나를 지운다(되살렸거나 버렸거나 이미 반영됐다). */
export function removeFormDraftAt(key: string): void {
  writeFormDraftAt(key, undefined);
}

/**
 * 되살릴지 물을 입력이 이 탭의 키에 있으면 보류 키로 옮긴다(묻는 동안의 새 입력이 그 키를 덮지 않게). 옮기지 못하면(저장소
 * 가득 참) 그대로 돌려준다. 부르는 쪽은 돌려받은 키가 아직 이 탭의 키이면 고를 때까지 그 키에 쓰지 않는다.
 */
export function holdFormDraft<T extends { savedAt: string }>(
  name: string,
  entry: StoredFormDraft<T>,
  userId = me.userId,
): StoredFormDraft<T> {
  const b = base(name, userId);
  if (b === undefined || entry.key !== formDraftTabKey(name, userId)) return entry;
  const held = `${b}:held:${crypto.randomUUID()}`;
  if (!writeFormDraftAt(held, entry.value)) return entry;
  writeFormDraftAt(entry.key, undefined);
  return { key: held, value: entry.value };
}
