import { type CommentView } from '$lib/features/activity/api';
import { describe, expect, it } from 'vitest';

import { threads } from '$lib/features/comments/threads';

const c = (id: string, parent: string | null, at: string): CommentView => ({
  id,
  parent_id: parent,
  body: id,
  author_id: 'u',
  author_name: 'u',
  created_at: at,
  edited: false,
  deleted: false,
  resolved: false,
  can_edit: false,
  can_delete: false,
  can_resolve: false,
});

describe('comment threads', () => {
  it('groups replies under their first comment in time order', () => {
    const t = threads([
      c('b2', 'b', '2026-10-17T03:00:00Z'),
      c('a', null, '2026-10-17T01:00:00Z'),
      c('b', null, '2026-10-17T02:00:00Z'),
      c('b1', 'b', '2026-10-17T02:30:00Z'),
    ]);
    expect(t.map((x) => [x.root.id, x.replies.map((r) => r.id)])).toEqual([
      ['a', []],
      ['b', ['b1', 'b2']],
    ]);
  });
});
