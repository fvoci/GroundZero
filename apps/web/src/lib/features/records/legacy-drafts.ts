import { writeDraftAt } from '$lib/features/common/draft-storage';
import { object, retry } from '$lib/features/modules/draft-values';
import longTextKeys from './long-text-keys.json';
import { parseRecordForm, recordWriting, type RecordForm } from './records';

const RECORD_DRAFT_KEY = /^fvoci:[^:]+:[^:]+:form-draft:record:/;
const RECORD_KIND_KEY =
  /^fvoci:[^:]+:[^:]+:form-draft:record:[^:]+:([^:]+):[^:]+:(?:page:[^:]+:)?(?:tab|held):[^:]+$/;
const LONG_TEXT_KEYS: Readonly<Record<string, readonly string[]>> = longTextKeys;
// Bound synchronous cleanup per key while allowing a transient competing writer to settle.
const RECORD_DRAFT_CLEANUP_ATTEMPTS = 3;

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

/** Normalize every read before it can be held or restored, even if startup cleanup missed its key. */
export function normalizeRecordDraft(
  kind: string | undefined,
  input: unknown,
): RecordDraftInput | undefined {
  const keys =
    kind !== undefined && Object.hasOwn(LONG_TEXT_KEYS, kind) ? LONG_TEXT_KEYS[kind] : undefined;
  if (keys === undefined) return undefined;
  try {
    if (
      !object(input) ||
      typeof input.baseline !== 'string' ||
      typeof input.version !== 'number' ||
      !Number.isSafeInteger(input.version) ||
      input.version < 0
    )
      return undefined;
    const form = parseRecordForm(input.form);
    const baseline = parseRecordForm(JSON.parse(input.baseline));
    if (form === undefined || baseline === undefined) return undefined;
    return {
      form: recordWriting(form, keys),
      baseline: JSON.stringify(recordWriting(baseline, keys)),
      version: input.version,
      // Old request JSON is never needed to recover writing or make a fresh request.
      pending: isRecordDraftPending(input.pending) ? input.pending : null,
    };
  } catch {
    return undefined;
  }
}

function checkWriting(value: unknown, kind: string | undefined): DraftCheck | undefined {
  if (!object(value) || typeof value.savedAt !== 'string') return undefined;
  const input = normalizeRecordDraft(kind, value.input);
  if (input === undefined) return undefined;
  const normalized = { savedAt: value.savedAt, input };
  return {
    state: JSON.stringify(normalized) === JSON.stringify(value) ? 'current' : 'rewrite',
    value: normalized,
  };
}

function storedStateAt(key: string, raw: string | null): 'same' | 'changed' | 'unavailable' {
  try {
    return window.localStorage.getItem(key) === raw ? 'same' : 'changed';
  } catch {
    return 'unavailable';
  }
}

/** Inspection never writes another tab's key. Changed snapshots need another inspection. */
function clearLegacyRecordDraftAt(key: string, raw: string | null): RecordDraftCleanup | 'changed' {
  if (!RECORD_DRAFT_KEY.test(key)) return 'ready';
  const stored = storedStateAt(key, raw);
  if (stored !== 'same') return stored === 'changed' ? 'changed' : 'retry';
  return 'ready';
}

/**
 * D-27, decisions C/B: inspect storage without rewriting keys. Legacy sources remain for explicit
 * recovery/discard; normalization happens only when reading writing into the current tab's form.
 */
export function clearLegacyRecordDrafts(): RecordDraftCleanup {
  const keys = recordDraftKeys();
  if (keys === undefined) return 'retry';
  let state: RecordDraftCleanup = 'ready';
  for (const key of keys) {
    try {
      let checked: RecordDraftCleanup | 'changed' = 'changed';
      for (
        let attempt = 0;
        attempt < RECORD_DRAFT_CLEANUP_ATTEMPTS && checked === 'changed';
        attempt += 1
      ) {
        checked = clearLegacyRecordDraftAt(key, window.localStorage.getItem(key));
      }
      if (checked !== 'ready') state = 'retry';
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
export function attachRecordDraftCleanup(onChange: () => void = () => undefined): () => void {
  function refresh(): void {
    clearLegacyRecordDrafts();
    onChange();
  }
  function onVisible(): void {
    if (document.visibilityState === 'visible') refresh();
  }
  function onStorage(event: StorageEvent): void {
    if (!isRecordDraftStorageEvent(event) || event.key === null) return;
    // Observe this event without rewriting its source, even if its owner has already closed.
    clearLegacyRecordDraftAt(event.key, event.newValue);
    onChange();
  }
  window.addEventListener('storage', onStorage);
  window.addEventListener('focus', refresh);
  document.addEventListener('visibilitychange', onVisible);
  return () => {
    window.removeEventListener('storage', onStorage);
    window.removeEventListener('focus', refresh);
    document.removeEventListener('visibilitychange', onVisible);
  };
}

export interface LegacyRecordDraft {
  key: string;
  raw: string;
}

/** Only the signed-in owner can see a content-free discard proposal, across their organizations. */
export function legacyRecordDrafts(userId: string): LegacyRecordDraft[] {
  if (userId === '') return [];
  const entries: LegacyRecordDraft[] = [];
  for (const key of recordDraftKeys() ?? []) {
    if (!key.startsWith(`fvoci:${userId}:`)) continue;
    try {
      const raw = window.localStorage.getItem(key);
      if (raw === null) continue;
      let checked: DraftCheck | undefined;
      try {
        checked = checkWriting(JSON.parse(raw), RECORD_KIND_KEY.exec(key)?.[1]);
      } catch {
        // Never interpolate malformed input or its key into the DOM.
      }
      if (checked?.state !== 'current') entries.push({ key, raw });
    } catch {
      // A later focus/storage event retries unreadable keys without removing them.
    }
  }
  return entries;
}

/** A changed snapshot needs another human choice; storage refusal retains the proposal. */
export function discardLegacyRecordDraft(entry: LegacyRecordDraft, userId: string): void {
  if (
    userId === '' ||
    !entry.key.startsWith(`fvoci:${userId}:`) ||
    !RECORD_DRAFT_KEY.test(entry.key)
  )
    return;
  if (storedStateAt(entry.key, entry.raw) === 'same') writeDraftAt(entry.key, undefined);
}
