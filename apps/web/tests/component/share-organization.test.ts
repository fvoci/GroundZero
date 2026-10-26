import { afterEach, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';

afterEach(() => {
  vi.unstubAllGlobals();
});

it('sends the selected organization when sharing a document', async () => {
  const fetch = vi.fn().mockResolvedValue(
    new Response(
      JSON.stringify({
        id: 'share-1',
        email: 'reader@example.test',
        display_name: 'Reader',
        permission: 'read',
      }),
      { status: 201, headers: { 'Content-Type': 'application/json' } },
    ),
  );
  vi.stubGlobal('fetch', fetch);
  await api.share('doc-1', 'reader@example.test', 'read', 'org-1');
  expect(fetch).toHaveBeenCalledWith(
    '/api/documents/doc-1/shares',
    expect.objectContaining({
      method: 'POST',
      body: JSON.stringify({
        email: 'reader@example.test',
        permission: 'read',
        organization_id: 'org-1',
      }),
    }),
  );
});
