import { untrack } from 'svelte';
import { SvelteDate } from 'svelte/reactivity';

import { me } from '$lib/me.svelte';

import {
  formDraftTabKey,
  holdFormDraft,
  listFormDrafts,
  removeFormDraftAt,
  type StoredFormDraft,
  writeFormDraftAt,
} from './form-draft';
import { createLeaveGuard } from './leave-guard.svelte';

interface DraftOptions<T> {
  /** Stable form kind and container/parent ID; undefined disables preservation. */
  name: () => string | undefined;
  snapshot: () => T;
  parse: (value: unknown) => T | undefined;
  restore: (value: T) => void;
  /** Clear visible input when its owner or target changes. */
  reset: () => void;
  dirty: () => boolean;
  busy?: () => boolean;
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

  function current(): boolean {
    return session?.userId === me.userId && session.name === options.name();
  }

  function persist(): void {
    if (!current() || session === undefined) return;
    // A failed move to a held key must never overwrite the offered input.
    if (offers.some((entry) => entry.key === session?.key)) return;
    writeFormDraftAt(
      session.key,
      options.dirty()
        ? { savedAt: new SvelteDate().toISOString(), input: options.snapshot() }
        : undefined,
    );
  }

  const leave = createLeaveGuard({
    dirty: () => current() && options.dirty(),
    busy: () => current() && options.busy?.() === true,
    identity: () => session,
    onAsk: persist,
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
        session === undefined
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
    const stored = session !== undefined && writeFormDraftAt(session.key, entry.value);
    restoredKey = stored ? undefined : entry.key;
    offers = offers.filter((candidate) => candidate.key !== entry.key);
    if (stored && entry.key !== session?.key) removeFormDraftAt(entry.key);
    persist();
  }

  function capture(): FormDraftAttempt | undefined {
    if (!current() || session === undefined || offers.length > 0) return undefined;
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
        return current() && options.dirty();
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
