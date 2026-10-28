import { readDraftAt, writeDraftAt } from '$lib/features/common/draft-storage';
import { object, retry } from '$lib/features/modules/draft-values';
import { parseRecordForm } from './records';

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

function hasKeys(value: Record<string, unknown>, keys: string[]): boolean {
  return (
    Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key))
  );
}

function isCurrentForm(value: unknown): boolean {
  const parsed = parseRecordForm(value);
  return (
    object(value) &&
    parsed !== undefined &&
    hasKeys(value, Object.keys(parsed)) &&
    Array.isArray(value.files) &&
    value.files.every(
      (file: unknown) => object(file) && hasKeys(file, ['file_id', 'version', 'role']),
    )
  );
}

function isCurrentDraft(value: unknown): boolean {
  if (
    !object(value) ||
    !hasKeys(value, ['savedAt', 'input']) ||
    typeof value.savedAt !== 'string' ||
    !object(value.input)
  )
    return false;
  const input = value.input;
  if (
    !hasKeys(input, ['form', 'baseline', 'version', 'pending']) ||
    !isCurrentForm(input.form) ||
    typeof input.baseline !== 'string' ||
    !isCurrentForm(JSON.parse(input.baseline)) ||
    typeof input.version !== 'number' ||
    !Number.isSafeInteger(input.version) ||
    input.version < 0
  )
    return false;
  const pending = input.pending;
  return (
    pending === null ||
    (retry(pending) &&
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(pending.requestId) &&
      /^[0-9a-f]{64}$/.test(pending.signature))
  );
}

/**
 * D-27, SR-199-1: old retries contain the complete request. Remove them before any screen opens,
 * across accounts, organizations, tabs and held offers. Snapshot keys before deleting so adjacent
 * entries cannot be skipped. SR-199-2: only verified current forms may survive, including unsent
 * writing with null pending. The shared reader returns undefined on JSON or schema parsing errors.
 */
export function clearLegacyRecordDrafts(): void {
  for (const key of recordDraftKeys()) {
    if (readDraftAt(key, isCurrentDraft) !== true) writeDraftAt(key, undefined);
  }
}
