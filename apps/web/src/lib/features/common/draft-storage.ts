/** 폼과 CSV 초안의 저장·열거·보류를 한 곳에서 처리한다. 키는 호출부가 사용자·조직·탭 문맥으로 정한다. */
export interface StoredDraft<T> {
  key: string;
  value: T;
}

export function readDraftAt<T>(
  key: string,
  parse: (value: unknown) => T | undefined,
): T | undefined {
  try {
    const raw = window.localStorage.getItem(key);
    return raw === null ? undefined : parse(JSON.parse(raw));
  } catch {
    return undefined;
  }
}

export function writeDraftAt(key: string | undefined, value: unknown): boolean {
  if (key === undefined) return false;
  try {
    if (value === undefined) window.localStorage.removeItem(key);
    else window.localStorage.setItem(key, JSON.stringify(value));
    return true;
  } catch {
    return false;
  }
}

export function listDraftsAt<T extends { savedAt: string }>(
  prefix: string,
  parse: (value: unknown) => T | undefined,
): StoredDraft<T>[] {
  const entries: StoredDraft<T>[] = [];
  try {
    for (let i = 0; i < window.localStorage.length; i += 1) {
      const key = window.localStorage.key(i);
      if (!key?.startsWith(prefix)) continue;
      const value = readDraftAt(key, parse);
      if (value !== undefined) entries.push({ key, value });
    }
  } catch {
    return [];
  }
  return entries.sort((a, b) => b.value.savedAt.localeCompare(a.value.savedAt));
}

export function holdDraftAt<T>(
  tabKey: string | undefined,
  heldPrefix: string,
  entry: StoredDraft<T>,
): StoredDraft<T> {
  if (entry.key !== tabKey) return entry;
  const key = `${heldPrefix}:${crypto.randomUUID()}`;
  if (!writeDraftAt(key, entry.value)) return entry;
  writeDraftAt(entry.key, undefined);
  return { key, value: entry.value };
}
