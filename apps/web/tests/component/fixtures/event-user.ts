import { beforeEach } from 'vitest';
import { me } from '$lib/me.svelte';

// Event forms now preserve drafts under the authenticated user, like the app shell.
beforeEach(() => {
  localStorage.clear();
  sessionStorage.clear();
  me.userId = 'event-test-user';
});
