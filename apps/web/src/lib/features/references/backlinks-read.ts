import type { BacklinkView } from '$lib/api/client';

/** Settle immediately so an early failure can wait for the screen's validation. */
export type BacklinksRead = Promise<{ kind: 'loaded'; links: BacklinkView[] } | { kind: 'failed' }>;

export function settleBacklinks(request: Promise<BacklinkView[]>): BacklinksRead {
  return request.then(
    (links) => ({ kind: 'loaded', links }),
    () => ({ kind: 'failed' }),
  );
}
