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
  /** Record recovery leaves every source key intact until a person discards it (C/B, #227). */
  retainSource?: boolean;
  /** Stable form kind and container/parent ID; undefined disables preservation. */
  name: () => string | undefined;
  snapshot: () => T;
  /** 저장할 칸만 고른다. 요청 완료 여부는 전체 snapshot으로 비교한다(D-27). */
  storedSnapshot?: () => T;
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
  let hiddenOffers = $state<StoredFormDraft<Saved<T>>[]>([]);
  let kept = $state(false);
  let disposed = false;

  function owned(): boolean {
    return session?.userId === me.userId && session.name === options.name();
  }

  function current(): boolean {
    return !disposed && owned() && !accountSwitching();
  }

  function blocked(): boolean {
    return offers.some((entry) => entry.key === session?.key && !hiddenOffers.includes(entry));
  }

  function writableKey(name: string, userId: string): string | undefined {
    const key = formDraftTabKey(name, userId);
    if (key === undefined || options.retainSource !== true) return key;
    try {
      if (localStorage.getItem(key) === null) return key;
    } catch {
      // Unknown storage may still contain an original; reserve a fresh key instead of overwriting it.
    }
    return `${key}-${crypto.randomUUID()}`;
  }

  function storedSnapshot(): T {
    return options.storedSnapshot === undefined ? options.snapshot() : options.storedSnapshot();
  }

  function persist(): boolean {
    if (!owned() || session === undefined) return false;
    if (options.preserve === false) {
      kept = !options.dirty();
      return kept;
    }
    // A failed move to a held key must never overwrite the offered input.
    if (offers.some((entry) => entry.key === session?.key)) {
      kept = !options.dirty();
      return false;
    }
    const dirty = options.dirty();
    const written = writeFormDraftAt(
      session.key,
      dirty ? { savedAt: new SvelteDate().toISOString(), input: storedSnapshot() } : undefined,
    );
    kept = !dirty || written;
    return written;
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
      hiddenOffers = [];
      session =
        name === undefined || userId === ''
          ? undefined
          : {
              userId,
              name,
              key: writableKey(name, userId),
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
            ).map((entry) =>
              options.retainSource === true ? entry : holdFormDraft(name ?? '', entry, userId),
            );
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
      const expected = JSON.stringify(storedSnapshot());
      if (!persist()) return;
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
          if (JSON.stringify(raw.input) !== expected) return undefined;
          const input = options.parse(raw.input);
          return input === undefined ? undefined : { savedAt: raw.savedAt, input };
        },
        session.userId,
      ).find((candidate) => candidate.key === session?.key);
      if (typed !== undefined) {
        const held =
          options.retainSource === true
            ? typed
            : holdFormDraft(session.name, typed, session.userId);
        if (options.retainSource === true) {
          session.key = `${session.key ?? ''}-${crypto.randomUUID()}`;
        }
        if (held.key === session.key) return;
        offers = [...offers, held];
      } else {
        // No durable copy was possible: retain the live input until it is discarded.
        kept = false;
        return;
      }
    }
    // B: keep the offered source for another proposal, even after a successful copy or completion.
    options.restore(entry.value.input);
    const value =
      options.storedSnapshot === undefined
        ? entry.value
        : { ...entry.value, input: options.storedSnapshot() };
    if (session !== undefined) writeFormDraftAt(session.key, value);
    offers = offers.filter((candidate) => candidate.key !== entry.key);
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
      writeFormDraftAt(session?.key, undefined);
    }
    options.reset();
    return true;
  }

  function discardAndLeave(): void {
    if (!leave.open || !current() || options.busy?.() === true) return;
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
    get key() {
      return session?.key;
    },
    get offers() {
      return offers.filter((entry) => !hiddenOffers.includes(entry));
    },
    /** Confirmed server copies stay protected in storage even if holding their own key failed. */
    hide(entry: StoredFormDraft<Saved<T>>) {
      if (current() && offers.includes(entry) && !hiddenOffers.includes(entry)) {
        hiddenOffers = [...hiddenOffers, entry];
      }
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
