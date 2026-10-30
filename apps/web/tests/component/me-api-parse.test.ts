import { expect, it } from 'vitest';

import { parseMe } from '$lib/api/client';

const account = {
  id: 'user-a',
  display_name: 'Current user',
  limits: {
    max_title_chars: 200,
    max_document_bytes: 2_000_000,
    max_references_per_document: 500,
    max_document_depth: 10,
  },
};

it.each([true, false])('reads is_server_admin=%s from the current account', (isServerAdmin) => {
  expect(parseMe({ ...account, is_server_admin: isServerAdmin })).toMatchObject({
    is_server_admin: isServerAdmin,
  });
});

it('rejects a current account without is_server_admin', () => {
  expect(() => parseMe(account)).toThrow('is_server_admin');
});

it.each([null, 'true', 'false', 0, 1])('rejects non-boolean is_server_admin=%s', (value) => {
  expect(() => parseMe({ ...account, is_server_admin: value })).toThrow('is_server_admin');
});
