import { readDraftAt, writeDraftAt } from '$lib/features/common/draft-storage';
import { object, retry } from '$lib/features/modules/draft-values';
import longTextKeys from './long-text-keys.json';
import { parseRecordForm, recordWriting, type RecordForm } from './records';

const RECORD_DRAFT_KEY = /^fvoci:[^:]+:[^:]+:form-draft:record:/;
const RECORD_KIND_KEY =
  /^fvoci:[^:]+:[^:]+:form-draft:record:[^:]+:([^:]+):[^:]+:(?:tab|held):[^:]+$/;
const LONG_TEXT_KEYS: Readonly<Record<string, readonly string[]>> = longTextKeys;

interface RecordDraftInput {
  form: RecordForm;
  baseline: string;
  version: number;
  pending: { requestId: string; signature: string } | null;
}

interface RecordDraft {
  savedAt: string;
  input: RecordDraftInput;
}

interface DraftCheck {
  state: 'current' | 'rewrite';
  value: RecordDraft;
}

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

function hasKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  return (
    Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key))
  );
}

/** Stored retries carry only the request UUID and digest, including on the panel read path. */
export function isRecordDraftPending(
  value: unknown,
): value is { requestId: string; signature: string } | null {
  return (
    value === null ||
    (retry(value) &&
      hasKeys(value, ['requestId', 'signature']) &&
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value.requestId) &&
      /^[0-9a-f]{64}$/.test(value.signature))
  );
}

function isRecordForm(value: unknown): boolean {
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

function isRecordDraftInput(input: unknown): input is RecordDraftInput {
  if (
    !object(input) ||
    !hasKeys(input, ['form', 'baseline', 'version', 'pending']) ||
    !isRecordForm(input.form) ||
    typeof input.baseline !== 'string' ||
    !isRecordForm(JSON.parse(input.baseline)) ||
    typeof input.version !== 'number' ||
    !Number.isSafeInteger(input.version) ||
    input.version < 0
  )
    return false;
  return isRecordDraftPending(input.pending);
}

function isWritingOnly(value: RecordForm, keys: readonly string[]): boolean {
  return (
    value.title === '' &&
    value.occurred_on === '' &&
    value.files.length === 0 &&
    Object.keys(value.people).length === 0 &&
    Object.keys(value.unreadable).length === 0 &&
    hasKeys(value.values, keys)
  );
}

/** Normalize every read before it can be held or restored, even if startup cleanup missed its key. */
export function normalizeRecordDraft(
  kind: string | undefined,
  input: unknown,
): RecordDraftInput | undefined {
  const keys =
    kind !== undefined && Object.hasOwn(LONG_TEXT_KEYS, kind) ? LONG_TEXT_KEYS[kind] : undefined;
  if (keys === undefined) return undefined;
  try {
    if (!isRecordDraftInput(input)) return undefined;
    const baseline = parseRecordForm(JSON.parse(input.baseline));
    if (baseline === undefined) return undefined;
    if (isWritingOnly(input.form, keys) && isWritingOnly(baseline, keys)) return input;
    return {
      form: recordWriting(input.form, keys),
      baseline: JSON.stringify(recordWriting(baseline, keys)),
      version: input.version,
      pending: input.pending,
    };
  } catch {
    return undefined;
  }
}

function checkWriting(value: unknown, kind: string | undefined): DraftCheck | undefined {
  if (!object(value) || !hasKeys(value, ['savedAt', 'input']) || typeof value.savedAt !== 'string')
    return undefined;
  const input = normalizeRecordDraft(kind, value.input);
  if (input === undefined) return undefined;
  return {
    state: input === value.input ? 'current' : 'rewrite',
    value: { savedAt: value.savedAt, input },
  };
}

/**
 * D-27, SR-199-1: old retries contain the complete request. Remove them before any screen opens,
 * across accounts, organizations, tabs and held offers. Snapshot keys before deleting so adjacent
 * entries cannot be skipped. Verify retry metadata, then normalize both form and baseline with the
 * offline allowlist checked against server kinds. Unknown kinds and unreadable values are discarded.
 */
function clearLegacyRecordDraftAt(key: string): void {
  if (!RECORD_DRAFT_KEY.test(key)) return;
  const kind = RECORD_KIND_KEY.exec(key)?.[1];
  const checked = readDraftAt(key, (value) => checkWriting(value, kind));
  if (checked === undefined) {
    writeDraftAt(key, undefined);
  } else if (checked.state === 'rewrite' && !writeDraftAt(key, checked.value)) {
    // A failed rewrite must not leave the old short-field snapshot on disk.
    writeDraftAt(key, undefined);
  }
}

export function clearLegacyRecordDrafts(): void {
  for (const key of recordDraftKeys()) clearLegacyRecordDraftAt(key);
}

/** Other tabs can still run an older release; attach for the App lifetime. */
export function attachRecordDraftCleanup(): () => void {
  function onStorage(event: StorageEvent): void {
    if (event.key === null) return;
    try {
      if (event.storageArea !== window.localStorage) return;
    } catch {
      return;
    }
    // Re-read the current value, since the event's snapshot can already be outdated.
    clearLegacyRecordDraftAt(event.key);
  }
  window.addEventListener('storage', onStorage);
  return () => {
    window.removeEventListener('storage', onStorage);
  };
}
