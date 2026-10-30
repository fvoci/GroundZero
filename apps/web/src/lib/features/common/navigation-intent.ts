import { untrack } from 'svelte';

import { api, ApiError, type ItemView } from '$lib/api/client';
import { displaySettings } from '$lib/display';
import { accountSafety, accountSwitching, auth } from '$lib/features/feedback/feedback.svelte';
import { homeApi, type MyTasksView } from '$lib/features/home/api';
import { dayIn } from '$lib/features/events/time';
import { me } from '$lib/me.svelte';
import { org } from '$lib/org.svelte';
import { m } from '$lib/paraglide/messages.js';

import { itemIntent, myTasksIntent } from './intent-data';

function identity(): string | undefined {
  // These safety reads must not become dependencies of the destination's load effect (D-96).
  return untrack(() =>
    me.userId === '' || org.currentId === undefined || accountSwitching()
      ? undefined
      : JSON.stringify([me.userId, org.currentId, accountSafety.generation]),
  );
}

function savesData(): boolean {
  return (
    'connection' in navigator &&
    typeof navigator.connection === 'object' &&
    navigator.connection !== null &&
    'saveData' in navigator.connection &&
    navigator.connection.saveData === true
  );
}

function assertCurrent(owner: string): void {
  if (identity() !== owner)
    throw new ApiError(409, 'account_changed', m.common_error_account_changed());
}

async function consume<T>(
  owner: string,
  pending: Promise<T | undefined>,
  fresh: () => Promise<T>,
): Promise<T> {
  const value = await pending;
  assertCurrent(owner);
  return value === undefined || untrack(() => auth.expired) ? fresh() : value;
}

/** Only the home -> my-tasks intent calls this; one organization/date-scoped GET. */
export function prefetchMyTasks(): void {
  const owner = identity();
  const organizationId = org.currentId;
  if (
    owner === undefined ||
    organizationId === undefined ||
    untrack(() => auth.expired) ||
    savesData()
  )
    return;
  const today = dayIn(new Date().toISOString(), displaySettings().timeZone);
  myTasksIntent.prefetch(JSON.stringify([owner, organizationId, today]), () =>
    homeApi.myTasks(organizationId, today),
  );
}

export function readMyTasks(organizationId: string, today: string): Promise<MyTasksView> {
  const owner = identity();
  const fresh = (): Promise<MyTasksView> => homeApi.myTasks(organizationId, today);
  if (owner === undefined || untrack(() => auth.expired)) return fresh();
  const pending = myTasksIntent.take(JSON.stringify([owner, organizationId, today]));
  return pending === undefined ? fresh() : consume(owner, pending, fresh);
}

/** Only module item-list links call this; no module, documents, tasks or recurrence writes. */
export function prefetchItem(id: string): void {
  const owner = identity();
  if (owner === undefined || untrack(() => auth.expired) || savesData()) return;
  itemIntent.prefetch(JSON.stringify([owner, id]), () => api.item(id));
}

export function readItem(id: string): Promise<ItemView> {
  const owner = identity();
  const fresh = (): Promise<ItemView> => api.item(id);
  if (owner === undefined || untrack(() => auth.expired)) return fresh();
  const pending = itemIntent.take(JSON.stringify([owner, id]));
  return pending === undefined ? fresh() : consume(owner, pending, fresh);
}

/** Add listeners alongside existing link/tooltip handlers, without replacing their props. */
export function prefetchOnIntent(
  node: HTMLAnchorElement,
  read: () => void,
): { destroy: () => void } {
  node.addEventListener('mouseenter', read);
  node.addEventListener('focus', read);
  node.addEventListener('touchstart', read, { passive: true });
  return {
    destroy: () => {
      node.removeEventListener('mouseenter', read);
      node.removeEventListener('focus', read);
      node.removeEventListener('touchstart', read);
    },
  };
}
