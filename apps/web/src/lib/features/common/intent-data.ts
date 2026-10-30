// #211 A experiment: one read per destination, kept briefly and consumed once.
// No HTTP cache, persistent storage, write request, or background refresh.
import type { ItemView } from '$lib/api/client';
import type { MyTasksView } from '$lib/features/home/api';

const LIFETIME_MS = 5_000;

class IntentRead<T> {
  private entry: { key: string; expires: number; promise: Promise<T | undefined> } | undefined;
  private timer: ReturnType<typeof setTimeout> | undefined;

  prefetch(key: string, read: () => Promise<T>): void {
    if (this.entry?.key === key && this.entry.expires > Date.now()) return;
    this.clear();
    const expires = Date.now() + LIFETIME_MS;
    this.entry = {
      key,
      expires,
      // A failed speculative read is retried by the destination's normal error path.
      promise: read()
        .then((value) => (Date.now() < expires ? value : undefined))
        .catch(() => undefined),
    };
    this.timer = setTimeout(() => {
      this.clear();
    }, LIFETIME_MS);
  }

  take(key: string): Promise<T | undefined> | undefined {
    const entry = this.entry;
    this.clear();
    return entry?.key === key && entry.expires > Date.now() ? entry.promise : undefined;
  }

  clear(): void {
    if (this.timer !== undefined) clearTimeout(this.timer);
    this.timer = undefined;
    this.entry = undefined;
  }
}

export const myTasksIntent = new IntentRead<MyTasksView>();
export const itemIntent = new IntentRead<ItemView>();

/** Called synchronously at the existing account-safety boundary, including same-user restore. */
export function clearIntentData(): void {
  myTasksIntent.clear();
  itemIntent.clear();
}
