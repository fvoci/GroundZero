import { onDestroy, untrack } from 'svelte';
import { SvelteDate } from 'svelte/reactivity';

import { me } from '$lib/me.svelte';
import { accountSwitching } from '$lib/features/feedback/feedback.svelte';

import {
  formDraftTabKey,
  holdFormDraft,
  listFormDrafts,
  removeFormDraftAt,
  type StoredFormDraft,
  writeFormDraftAt,
} from './form-draft';
import { createLeaveGuard, leavingWithDrafts } from './leave-guard.svelte';

interface DraftOptions<T> {
  /** 짧은 설정 칸은 화면에만 두고 이탈을 경고한다(D-27·D-97). */
  preserve?: boolean;
  /** Stable form kind and container/parent ID; undefined disables preservation. */
  name: () => string | undefined;
  snapshot: () => T;
  /** 저장할 칸만 고른다. 요청 완료 여부는 전체 snapshot으로 비교한다(D-27). */
  storedSnapshot?: () => T;
  /** Observe successful writes to this session's key, including its exact savedAt and projection. */
  onPersist?: (key: string, value: Saved<T> | undefined) => void;
  parse: (value: unknown) => T | undefined;
  restore: (value: T) => void;
  /** Clear visible input when its owner or target changes. */
  reset: () => void;
  dirty: () => boolean;
  /**
   * Secret input (API keys, delete confirmations) that must never be stored. It only counts toward the
   * leave warning while on screen, so `snapshot`/`restore` never carry it (R3 A, D-96).
   */
  screenOnly?: () => boolean;
  busy?: () => boolean;
  /** The form stays mounted at this in-app path, so leaving for it does not ask (see createLeaveGuard). */
  stays?: (next: string) => boolean;
}

interface Saved<T> {
  savedAt: string;
  input: T;
}

/** An attempt belongs to exactly one user, target and input snapshot. */
export interface FormDraftAttempt {
  readonly userId: string;
  readonly session: object;
  readonly snapshot: string;
}

/** Explicit callbacks keep private input out of unrelated forms and DOM inspection. */
export function createFormDraft<T>(options: DraftOptions<T>) {
  let session = $state<{ userId: string; name: string; key: string | undefined }>();
  let offers = $state<StoredFormDraft<Saved<T>>[]>([]);
  let restoredKey: string | undefined;
  let kept = $state(false);
  let disposed = false;

  function owned(): boolean {
    return session?.userId === me.userId && session.name === options.name();
  }

  function current(): boolean {
    return !disposed && owned() && !accountSwitching();
  }

  function blocked(): boolean {
    return offers.some((entry) => entry.key === session?.key);
  }

  function writeCurrent(key: string | undefined, value: Saved<T> | undefined): boolean {
    const written = writeFormDraftAt(key, value);
    if (written && key !== undefined) options.onPersist?.(key, value);
    return written;
  }

  function persist(): void {
    if (!owned() || session === undefined) return;
    if (options.preserve === false) {
      kept = !options.dirty();
      return;
    }
    // A failed move to a held key must never overwrite the offered input.
    if (blocked()) {
      kept = false;
      return;
    }
    const dirty = options.dirty();
    const written = writeCurrent(
      session.key,
      dirty
        ? {
            savedAt: new SvelteDate().toISOString(),
            input:
              options.storedSnapshot === undefined ? options.snapshot() : options.storedSnapshot(),
          }
        : undefined,
    );
    kept = !dirty || written;
  }

  onDestroy(() => {
    disposed = true;
  });

  const held = (): boolean => options.screenOnly?.() === true;

  const leave = createLeaveGuard({
    dirty: () => current() && (options.dirty() || held()),
    busy: () => current() && options.busy?.() === true,
    identity: () => session,
    onAsk: persist,
    ...(options.stays === undefined ? {} : { stays: options.stays }),
  });

  $effect(() => {
    const userId = me.userId;
    const name = options.name();
    untrack(() => {
      if (session?.userId === userId && session.name === name) return;
      leave.keep();
      if (session !== undefined) options.reset();
      restoredKey = undefined;
      session =
        name === undefined || userId === ''
          ? undefined
          : {
              userId,
              name,
              key: formDraftTabKey(name, userId),
            };
      offers =
        session === undefined || options.preserve === false
          ? []
          : listFormDrafts(
              name ?? '',
              (raw): Saved<T> | undefined => {
                if (
                  typeof raw !== 'object' ||
                  raw === null ||
                  !('savedAt' in raw) ||
                  typeof raw.savedAt !== 'string' ||
                  !('input' in raw)
                )
                  return undefined;
                const input = options.parse(raw.input);
                return input === undefined ? undefined : { savedAt: raw.savedAt, input };
              },
              userId,
            ).map((entry) => holdFormDraft(name ?? '', entry, userId));
    });
  });

  $effect(() => {
    options.snapshot();
    options.dirty();
    persist();
  });

  $effect(() => {
    if (accountSwitching()) {
      untrack(persist);
      leave.keep();
    }
  });

  function discard(entry: StoredFormDraft<Saved<T>>): void {
    if (!current() || !offers.includes(entry)) return;
    removeFormDraftAt(entry.key);
    offers = offers.filter((candidate) => candidate.key !== entry.key);
    persist();
  }

  function restore(entry: StoredFormDraft<Saved<T>>): void {
    if (!current() || !offers.includes(entry)) return;
    // Restoring an older offer must also preserve newly typed input.
    if (options.dirty() && session !== undefined && entry.key !== session.key) {
      persist();
      const typed = listFormDrafts(
        session.name,
        (raw): Saved<T> | undefined => {
          if (
            typeof raw !== 'object' ||
            raw === null ||
            !('savedAt' in raw) ||
            typeof raw.savedAt !== 'string' ||
            !('input' in raw)
          )
            return undefined;
          const input = options.parse(raw.input);
          return input === undefined ? undefined : { savedAt: raw.savedAt, input };
        },
        session.userId,
      ).find((candidate) => candidate.key === session?.key);
      if (typed !== undefined) {
        const held = holdFormDraft(session.name, typed, session.userId);
        if (held.key === session.key) return;
        offers = [...offers, held];
      } else {
        // No durable copy was possible: retain the live input until it is discarded.
        return;
      }
    }
    // Keep the offered copy if storage fails; never destroy the only durable copy.
    options.restore(entry.value.input);
    const value =
      options.storedSnapshot === undefined
        ? entry.value
        : { ...entry.value, input: options.storedSnapshot() };
    const stored = session !== undefined && writeCurrent(session.key, value);
    restoredKey = stored ? undefined : entry.key;
    offers = offers.filter((candidate) => candidate.key !== entry.key);
    if (stored && entry.key !== session?.key) removeFormDraftAt(entry.key);
    persist();
  }

  function capture(): FormDraftAttempt | undefined {
    if (!current() || session === undefined || blocked()) return undefined;
    persist();
    return { userId: session.userId, session, snapshot: JSON.stringify(options.snapshot()) };
  }

  function isOwner(attempt: FormDraftAttempt): boolean {
    return current() && attempt.session === session;
  }

  function isCurrent(attempt: FormDraftAttempt): boolean {
    return isOwner(attempt) && attempt.snapshot === JSON.stringify(options.snapshot());
  }

  function complete(attempt: FormDraftAttempt): boolean {
    if (!isCurrent(attempt)) return false;
    if (!offers.some((entry) => entry.key === session?.key)) {
      writeCurrent(session?.key, undefined);
    }
    if (restoredKey !== undefined) removeFormDraftAt(restoredKey);
    restoredKey = undefined;
    options.reset();
    return true;
  }

  function discardAndLeave(): void {
    if (!leave.open || !current() || options.busy?.() === true) return;
    if (restoredKey !== undefined) removeFormDraftAt(restoredKey);
    restoredKey = undefined;
    options.reset();
    persist();
    leave.discard();
  }

  /** Attach in onMount; each caller owns its matching confirmation UI. */
  function attach(): () => void {
    return leave.attach({
      beforeUnload: () => {
        persist();
        // Screen-only input has no stored copy, so it is never "kept": it warns during an account
        // switch or a new-version reload too. Cancelling leaves the hidden safety state with the input
        // only in memory (D-96).
        return owned() && ((options.dirty() && (!leavingWithDrafts() || !kept)) || held());
      },
    });
  }

  return {
    get offers() {
      return offers;
    },
    get active() {
      return current();
    },
    get blocked() {
      return blocked();
    },
    /** 원 사용자 입력을 보존했는지. 계정 전환 reload 허용 여부를 부모가 판단한다. */
    get kept() {
      return owned() && (!options.dirty() || kept);
    },
    leave,
    persist,
    restore,
    discard,
    discardAndLeave,
    capture,
    isOwner,
    isCurrent,
    complete,
    attach,
  };
}
