import { readDraftAt, writeDraftAt } from '$lib/features/common/draft-storage';
import { object, retry } from '$lib/features/modules/draft-values';

const RECORD_DRAFT_KEY = /^fvoci:[^:]+:[^:]+:form-draft:record:/;

function recordDraftKeys(): string[] {
  const keys: string[] = [];
  try {
    const storage = window.localStorage;
    for (let index = 0; index < storage.length; index += 1) {
      const key = storage.key(index);
      if (key !== null && RECORD_DRAFT_KEY.test(key)) keys.push(key);
    }
  } catch {
    return [];
  }
  return keys;
}

function hasLegacyRetry(value: unknown): boolean {
  if (!object(value) || !object(value.input)) return false;
  const pending = value.input.pending;
  if (pending === null || pending === undefined) return false;
  return !retry(pending) || !/^[0-9a-f]{64}$/.test(pending.signature);
}

/**
 * D-27, SR-199-1: old retries contain the complete request. Remove them before any screen opens,
 * across accounts, organizations, tabs and held offers. Snapshot keys before deleting so adjacent
 * entries cannot be skipped. Current UUID/digest retries and unsent writing remain untouched.
 */
export function clearLegacyRecordDrafts(): void {
  for (const key of recordDraftKeys()) {
    if (readDraftAt(key, hasLegacyRetry) === true) writeDraftAt(key, undefined);
  }
}
