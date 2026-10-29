import type { RecordInput } from '$lib/api/client';

/** Retry identity must not retain the short fields of the original request in browser storage. */
export async function recordRequestSignature(
  item: string,
  kind: string,
  input: RecordInput,
): Promise<string> {
  const bytes = new TextEncoder().encode(JSON.stringify({ item, kind, input }));
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}
