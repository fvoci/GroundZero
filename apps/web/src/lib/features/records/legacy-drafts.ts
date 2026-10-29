import { writeDraftAt } from '$lib/features/common/draft-storage';
import { object, retry } from '$lib/features/modules/draft-values';
import longTextKeys from './long-text-keys.json';
import { parseRecordForm, recordWriting, type RecordForm } from './records';
import {
  hasUnsentRecordWriting,
  isCommittedRecordDraftAction,
  recordDraftActionTarget,
  recordDraftIdentity,
  recordDraftWasHandled,
} from './draft-actions';

const RECORD_DRAFT_KEY = /^fvoci:[^:]+:[^:]+:form-draft:record:/;
const RECORD_KIND_KEY =
  /^fvoci:[^:]+:[^:]+:form-draft:record:[^:]+:([^:]+):[^:]+:(?:tab|held):[^:]+$/;
const LONG_TEXT_KEYS: Readonly<Record<string, readonly string[]>> = longTextKeys;
// Bound synchronous cleanup per key while allowing a transient competing writer to settle.
const RECORD_DRAFT_CLEANUP_ATTEMPTS = 3;

export interface RecordDraftInput {
  form: RecordForm;
  baseline: string;
  version: number;
  pending: { requestId: string; signature: string } | null;
}

export interface RecordDraft {
  savedAt: string;
  input: RecordDraftInput;
}

interface DraftCheck {
  state: 'current' | 'rewrite';
  value: RecordDraft;
}

export type RecordDraftCleanup = 'ready' | 'retry';

interface CleanupWrite {
  before: string | null;
  after: string | null;
}

// One last mutation per key, held only for this page's lifetime. No raw cleanup history is persisted.
const cleanupWrites = new Map<string, CleanupWrite>();
const deferredStorageEvents = new Map<string, StorageEvent>();

function writeCleanup(key: string, before: string | null, value: RecordDraft | undefined): boolean {
  if (!writeDraftAt(key, value)) return false;
  cleanupWrites.set(key, { before, after: value === undefined ? null : JSON.stringify(value) });
  return true;
}

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

export function recordDraftAt(key: string, raw: string | null): RecordDraft | undefined {
  if (raw === null) return undefined;
  try {
    return checkWriting(JSON.parse(raw), RECORD_KIND_KEY.exec(key)?.[1])?.value;
  } catch {
    return undefined;
  }
}

export function recordDraftInputAt(key: string, raw: string | null): RecordDraftInput | undefined {
  return recordDraftAt(key, raw)?.input;
}

function storedStateAt(key: string, raw: string | null): 'same' | 'changed' | 'unavailable' {
  try {
    return window.localStorage.getItem(key) === raw ? 'same' : 'changed';
  } catch {
    return 'unavailable';
  }
}

/** Inspect one snapshot without overwriting a different one; full scans retry changed keys. */
function clearLegacyRecordDraftAt(key: string, raw: string | null): RecordDraftCleanup | 'changed' {
  if (!RECORD_DRAFT_KEY.test(key)) return 'ready';
  const kind = RECORD_KIND_KEY.exec(key)?.[1];
  let checked: DraftCheck | undefined;
  try {
    if (raw !== null) checked = checkWriting(JSON.parse(raw), kind);
  } catch {
    // Malformed JSON is discarded only if this exact raw value is still stored.
  }
  const stored = storedStateAt(key, raw);
  if (stored !== 'same') return stored === 'changed' ? 'changed' : 'retry';
  if (raw === null || checked?.state === 'current') return 'ready';
  // localStorage has no atomic compare-and-set across tabs. Compare just before each mutation.
  if (writeCleanup(key, raw, checked?.value)) return 'ready';
  if (checked === undefined) return 'retry';
  const fallback = storedStateAt(key, raw);
  if (fallback !== 'same') return fallback === 'changed' ? 'changed' : 'retry';
  // A failed rewrite may discard only the old snapshot, never newer writing.
  return writeCleanup(key, raw, undefined) ? 'ready' : 'retry';
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

function savedTime(raw: string | null): number {
  if (raw === null) return Number.NEGATIVE_INFINITY;
  try {
    const value: unknown = JSON.parse(raw);
    const time =
      object(value) && typeof value.savedAt === 'string' ? Date.parse(value.savedAt) : Number.NaN;
    return Number.isFinite(time) ? time : Number.NEGATIVE_INFINITY;
  } catch {
    return Number.NEGATIVE_INFINITY;
  }
}

/** Recover a successor that was overwritten between our last comparison and mutation. */
export async function handleRecordDraftStorageEvent(
  event: StorageEvent,
): Promise<RecordDraftCleanup> {
  if (!isRecordDraftStorageEvent(event) || event.key === null) return 'ready';
  const key = event.key;
  const finish = (state: RecordDraftCleanup): RecordDraftCleanup => {
    if (state === 'retry') deferredStorageEvents.set(key, event);
    else if (deferredStorageEvents.get(key) === event) deferredStorageEvents.delete(key);
    return state;
  };
  const ordinary = (): RecordDraftCleanup =>
    finish(clearLegacyRecordDraftAt(key, event.newValue) === 'retry' ? 'retry' : 'ready');
  const receipt = cleanupWrites.get(key);
  const candidate = recordDraftAt(key, event.newValue);
  if (
    receipt === undefined ||
    candidate === undefined ||
    !hasUnsentRecordWriting(candidate.input) ||
    event.newValue === receipt.after
  )
    return ordinary();
  const previousTime = Math.max(savedTime(receipt.before), savedTime(receipt.after));
  const newerTime = Number.isFinite(previousTime) && savedTime(event.newValue) > previousTime;
  // Exact succession needs no clock; later timestamps also cover remove-then-set writers.
  if (event.oldValue !== receipt.before && !newerTime) return ordinary();
  const source = recordDraftAt(key, receipt.before);
  if (
    source !== undefined &&
    recordDraftIdentity(source.input) === recordDraftIdentity(candidate.input)
  )
    return ordinary();
  try {
    const handled = await recordDraftWasHandled(key, candidate.input, candidate.savedAt);
    if (cleanupWrites.get(key) !== receipt) return finish('ready');
    const stored = storedStateAt(key, receipt.after);
    if (stored === 'unavailable') return finish('retry');
    if (handled || stored === 'changed') return finish('ready');
    if (!writeCleanup(key, receipt.after, candidate)) return finish('retry');
    // A normal disposition may have committed between the preceding check and our write.
    const restored = JSON.stringify(candidate);
    if (
      (await recordDraftWasHandled(key, candidate.input, candidate.savedAt)) &&
      storedStateAt(key, restored) === 'same' &&
      !writeCleanup(key, restored, undefined)
    )
      return finish('retry');
    return finish('ready');
  } catch {
    return finish('retry');
  }
}

export function isRecordDraftActionEvent(event: StorageEvent): boolean {
  try {
    return (
      recordDraftActionTarget(event.key) !== undefined && event.storageArea === window.localStorage
    );
  } catch {
    return false;
  }
}

export function removeAcknowledgedRecordDraft(key: string, expected: string): RecordDraftCleanup {
  const stored = storedStateAt(key, expected);
  if (stored === 'unavailable') return 'retry';
  return stored === 'changed' || writeCleanup(key, expected, undefined) ? 'ready' : 'retry';
}

export async function retryRecordDraftStorageEvents(): Promise<RecordDraftCleanup> {
  let state: RecordDraftCleanup = 'ready';
  for (const event of [...deferredStorageEvents.values()])
    if (
      (isRecordDraftActionEvent(event)
        ? await handleRecordDraftActionEvent(event, false)
        : await handleRecordDraftStorageEvent(event)) === 'retry'
    )
      state = 'retry';
  return state;
}

/** A late acknowledgement also removes a recovery that raced with the normal removal. */
export async function handleRecordDraftActionEvent(
  event: StorageEvent,
  replayDeferred = true,
): Promise<RecordDraftCleanup> {
  if (!isRecordDraftActionEvent(event)) return 'ready';
  const key = recordDraftActionTarget(event.key);
  const eventKey = event.key;
  if (key === undefined || eventKey === null) return 'ready';
  const finish = (state: RecordDraftCleanup): RecordDraftCleanup => {
    if (state === 'retry') deferredStorageEvents.set(eventKey, event);
    else if (deferredStorageEvents.get(eventKey) === event) deferredStorageEvents.delete(eventKey);
    return state;
  };
  try {
    if (isCommittedRecordDraftAction(event.newValue)) {
      const raw = window.localStorage.getItem(key);
      const value = recordDraftAt(key, raw);
      if (value !== undefined && (await recordDraftWasHandled(key, value.input, value.savedAt))) {
        const stored = storedStateAt(key, raw);
        if (stored === 'unavailable') return finish('retry');
        if (stored === 'same' && !writeCleanup(key, raw, undefined)) return finish('retry');
      }
    }
    return finish(replayDeferred ? await retryRecordDraftStorageEvents() : 'ready');
  } catch {
    return finish('retry');
  }
}

/** Other tabs can still run an older release; attach for the App lifetime. */
export function attachRecordDraftCleanup(): () => void {
  const onReturn = (): void => {
    void retryRecordDraftStorageEvents().then(() => clearLegacyRecordDrafts());
  };
  function onVisible(): void {
    if (document.visibilityState === 'visible') onReturn();
  }
  function onStorage(event: StorageEvent): void {
    if (isRecordDraftActionEvent(event)) {
      void handleRecordDraftActionEvent(event);
      return;
    }
    if (!isRecordDraftStorageEvent(event) || event.key === null) return;
    // Reconcile our last cleanup before applying this event's normal writing projection.
    void handleRecordDraftStorageEvent(event);
  }
  window.addEventListener('storage', onStorage);
  window.addEventListener('focus', onReturn);
  document.addEventListener('visibilitychange', onVisible);
  return () => {
    window.removeEventListener('storage', onStorage);
    window.removeEventListener('focus', onReturn);
    document.removeEventListener('visibilitychange', onVisible);
    cleanupWrites.clear();
    deferredStorageEvents.clear();
  };
}
