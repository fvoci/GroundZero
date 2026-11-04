import type { EventRead, EventView } from '$lib/features/events/model';
import { me } from '$lib/me.svelte';

export function eventRead(event: EventView): EventRead {
  return { ...event, viewer_id: me.userId, archived: false };
}
