import { writeDraftAt } from '$lib/features/common/draft-storage';
import type { RecordDraftInput } from './legacy-drafts';
import { parseRecordForm, type RecordForm } from './records';

interface DraftAction {
  id: string;
  action: 'save' | 'restore' | 'discard';
  digest: string;
  through: number | null;
  state: 'pending' | 'handled';
}

export interface RecordDraftTarget {
  key: string;
  input: RecordDraftInput;
  savedAt?: string;
  through?: number;
}

function actionBase(key: string): string {
  const scoped = key.replace(/^(fvoci:[^:]+:[^:]+):form-draft:record:/, '$1:record-draft-action:');
  if (scoped === key) throw new Error('not a record draft key');
  return scoped;
}

export function recordDraftActionTarget(key: string | null): string | undefined {
  const match = key?.match(
    /^(fvoci:[^:]+:[^:]+):record-draft-action:(.+):[0-9a-f]{64}:(?:time|stamp:[0-9a-f]{64}):[0-9a-f-]{36}$/i,
  );
  const prefix = match?.[1];
  const suffix = match?.[2];
  return prefix === undefined || suffix === undefined
    ? undefined
    : prefix + ':form-draft:record:' + suffix;
}

function writing(value: RecordForm) {
  return [
    value.body,
    Object.entries(value.values).sort(([left], [right]) => left.localeCompare(right)),
  ];
}

function writingState(input: RecordDraftInput) {
  const baseline = parseRecordForm(JSON.parse(input.baseline));
  if (baseline === undefined) throw new Error('invalid record writing baseline');
  return [writing(input.form), writing(baseline), input.version];
}

/** Retry metadata fences asynchronous editing work but is not a different generation of the same writing. */
export function recordDraftIdentity(input: RecordDraftInput): string {
  return JSON.stringify([
    ...writingState(input),
    input.pending?.requestId ?? null,
    input.pending?.signature ?? null,
  ]);
}

export function hasUnsentRecordWriting(input: RecordDraftInput): boolean {
  const baseline = parseRecordForm(JSON.parse(input.baseline));
  return (
    baseline !== undefined &&
    JSON.stringify(writing(input.form)) !== JSON.stringify(writing(baseline))
  );
}

async function digest(identity: string): Promise<string> {
  const hash = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(identity));
  return [...new Uint8Array(hash)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

async function scope(target: RecordDraftTarget) {
  const time =
    target.through ?? (target.savedAt === undefined ? Date.now() : Date.parse(target.savedAt));
  const hash = await digest(JSON.stringify(writingState(target.input)));
  const prefix = actionBase(target.key) + ':' + hash + ':';
  if (Number.isSafeInteger(time)) return { prefix: prefix + 'time:', hash, through: time };
  return {
    prefix: prefix + 'stamp:' + (await digest(target.savedAt ?? '')) + ':',
    hash,
    through: null,
  };
}

function actionAt(raw: string | null): DraftAction | undefined {
  if (raw === null) return undefined;
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return undefined;
  }
  if (
    typeof value !== 'object' ||
    value === null ||
    !('id' in value) ||
    typeof value.id !== 'string' ||
    !/^[0-9a-f-]{36}$/i.test(value.id) ||
    !('action' in value) ||
    (value.action !== 'save' && value.action !== 'restore' && value.action !== 'discard') ||
    !('digest' in value) ||
    typeof value.digest !== 'string' ||
    !/^[0-9a-f]{64}$/.test(value.digest) ||
    !('through' in value) ||
    (value.through !== null &&
      (typeof value.through !== 'number' || !Number.isSafeInteger(value.through))) ||
    !('state' in value) ||
    (value.state !== 'pending' && value.state !== 'handled') ||
    Object.keys(value).length !== 5
  )
    return undefined;
  return {
    id: value.id,
    action: value.action,
    digest: value.digest,
    through: value.through,
    state: value.state,
  };
}

export function isCommittedRecordDraftAction(raw: string | null): boolean {
  return actionAt(raw)?.state === 'handled';
}

/** A later action never erases an earlier acknowledgement, and later authored timestamps are independent. */
export async function recordDraftWasHandled(
  key: string,
  input: RecordDraftInput,
  savedAt: string,
): Promise<boolean> {
  const candidate = await scope({ key, input, savedAt });
  const storage = window.localStorage;
  const length = storage.length;
  let pending = false;
  for (let index = 0; index < length; index += 1) {
    const name = storage.key(index);
    if (!name?.startsWith(candidate.prefix)) continue;
    const action = actionAt(storage.getItem(name));
    if (action?.digest !== candidate.hash) continue;
    const covered =
      candidate.through === null
        ? action.through === null
        : action.through !== null && action.through >= candidate.through;
    if (!covered) continue;
    if (action.state === 'handled') return true;
    pending = true;
  }
  if (storage.length !== length || pending)
    throw new DOMException('record draft disposition is not settled', 'InvalidStateError');
  return false;
}

/** Publish pending intent, then acknowledge the guarded normal action; failed actions roll back only their own marker. */
export async function runRecordDraftAction(
  targets: readonly RecordDraftTarget[],
  action: DraftAction['action'],
  current: () => boolean,
  run: () => boolean,
): Promise<boolean> {
  const markers = await Promise.all(
    targets.map(async (target) => {
      const point = await scope(target);
      const id = crypto.randomUUID();
      const marker: DraftAction = {
        id,
        action,
        digest: point.hash,
        through: point.through,
        state: 'pending',
      };
      return { key: point.prefix + id, marker };
    }),
  );
  if (!current()) return false;
  let completed = false;
  try {
    for (const { key, marker } of markers) {
      if (!writeDraftAt(key, marker))
        throw new DOMException('record draft action unavailable', 'SecurityError');
    }
    completed = run();
    if (completed) {
      for (const { key, marker } of markers) {
        if (!writeDraftAt(key, { ...marker, state: 'handled' }))
          throw new DOMException('record draft acknowledgement unavailable', 'SecurityError');
      }
    }
    return completed;
  } finally {
    if (!completed) for (const { key } of markers) writeDraftAt(key, undefined);
  }
}
