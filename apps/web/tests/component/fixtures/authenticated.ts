import { afterEach, beforeEach } from 'vitest';
import { me } from '$lib/me.svelte';

// These component fixtures represent an authenticated workspace editor.
beforeEach(() => {
  me.userId = 'user-a';
  localStorage.clear();
});
afterEach(() => {
  me.userId = '';
  localStorage.clear();
});
