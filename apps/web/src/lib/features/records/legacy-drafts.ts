import { writeDraftAt } from '$lib/features/common/draft-storage';
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

export type RecordDraftCleanup = 'ready' | 'retry';

function recordDraftKeys(): string[] | undefined {
  const keys: string[] = [];
  try {
    const storage = window.localStorage;
    for (let index = 0; index < storage.length; index += 1) {
      const key = storage.key(index);
      if (key !== null && RECORD_DRAFT_KEY.test(key)) keys.push(key);
    }
  } catch {
    return undefined;
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

function storedStateAt(key: string, raw: string): 'same' | 'changed' | 'unavailable' {
  try {
    return window.localStorage.getItem(key) === raw ? 'same' : 'changed';
  } catch {
    return 'unavailable';
  }
}

/** Normalize only the supplied snapshot; a later writer's value belongs to its own event. */
function clearLegacyRecordDraftAt(key: string, raw: string | null): RecordDraftCleanup {
  if (!RECORD_DRAFT_KEY.test(key) || raw === null) return 'ready';
  const kind = RECORD_KIND_KEY.exec(key)?.[1];
  let checked: DraftCheck | undefined;
  try {
    checked = checkWriting(JSON.parse(raw), kind);
  } catch {
    // Malformed JSON is discarded only if this exact raw value is still stored.
  }
  if (checked?.state === 'current') return 'ready';
  const stored = storedStateAt(key, raw);
  if (stored !== 'same') return stored === 'changed' ? 'ready' : 'retry';
  // localStorage has no atomic compare-and-set across tabs. Compare just before each mutation.
  if (writeDraftAt(key, checked?.value)) return 'ready';
  if (checked === undefined) return 'retry';
  const fallback = storedStateAt(key, raw);
  if (fallback !== 'same') return fallback === 'changed' ? 'ready' : 'retry';
  // A failed rewrite may discard only the old snapshot, never newer writing.
  return writeDraftAt(key, undefined) ? 'ready' : 'retry';
}

/**
 * D-27, SR-199-1: remove old raw retries before any screen opens, across accounts, organizations,
 * tabs and held offers. Snapshot keys so deleting an entry cannot skip the next one. Both startup
 * and event cleanup use the same writing normalization and compare before changing a stored value.
 */
export function clearLegacyRecordDrafts(): RecordDraftCleanup {
  const keys = recordDraftKeys();
  if (keys === undefined) return 'retry';
  let state: RecordDraftCleanup = 'ready';
  for (const key of keys) {
    try {
      if (clearLegacyRecordDraftAt(key, window.localStorage.getItem(key)) === 'retry')
        state = 'retry';
    } catch {
      // A failed read must not discard writing that could not be inspected.
      state = 'retry';
    }
  }
  return state;
}

export function isRecordDraftStorageEvent(event: StorageEvent): boolean {
  try {
    return (
      event.key !== null &&
      RECORD_DRAFT_KEY.test(event.key) &&
      event.storageArea === window.localStorage
    );
  } catch {
    return false;
  }
}

/** Other tabs can still run an older release; attach for the App lifetime. */
export function attachRecordDraftCleanup(): () => void {
  function onVisible(): void {
    if (document.visibilityState === 'visible') clearLegacyRecordDrafts();
  }
  function onStorage(event: StorageEvent): void {
    if (!isRecordDraftStorageEvent(event) || event.key === null) return;
    // Normalize this event's value; a newer value is left for its own event.
    clearLegacyRecordDraftAt(event.key, event.newValue);
  }
  window.addEventListener('storage', onStorage);
  window.addEventListener('focus', clearLegacyRecordDrafts);
  document.addEventListener('visibilitychange', onVisible);
  return () => {
    window.removeEventListener('storage', onStorage);
    window.removeEventListener('focus', clearLegacyRecordDrafts);
    document.removeEventListener('visibilitychange', onVisible);
  };
}
