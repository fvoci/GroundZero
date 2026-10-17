import { describe, expect, it } from 'vitest';

import { planMove, visibleRows } from '$lib/features/documents/tree';

// a
//   a1
//   a2
//     a2x
// b
// orphan (부모가 목록에 없음 = 볼 수 없는 부모) → 최상위
const docs = [
  { id: 'a', parent_id: null },
  { id: 'b', parent_id: null },
  { id: 'a1', parent_id: 'a' },
  { id: 'a2', parent_id: 'a' },
  { id: 'a2x', parent_id: 'a2' },
  { id: 'orphan', parent_id: 'hidden' },
];

describe('document tree', () => {
  it('flattens depth-first, keeps server order and roots orphans', () => {
    const rows = visibleRows(docs, new Set());
    expect(rows.map((r) => `${r.doc.id}:${String(r.depth)}`)).toEqual([
      'a:1',
      'a1:2',
      'a2:2',
      'a2x:3',
      'b:1',
      'orphan:1',
    ]);
    expect(rows[0]).toMatchObject({ hasChildren: true, setSize: 3, position: 1 });
    expect(visibleRows(docs, new Set(['a'])).map((r) => r.doc.id)).toEqual(['a', 'b', 'orphan']);
  });

  it('plans moves as parent and next sibling', () => {
    expect(planMove(docs, 'a2', 'up')).toEqual({ parentId: 'a', beforeId: 'a1' });
    expect(planMove(docs, 'a1', 'down')).toEqual({ parentId: 'a', beforeId: null });
    expect(planMove(docs, 'a', 'down')).toEqual({ parentId: null, beforeId: 'orphan' });
    expect(planMove(docs, 'a2', 'indent')).toEqual({ parentId: 'a1', beforeId: null });
    expect(planMove(docs, 'a2x', 'outdent')).toEqual({ parentId: 'a', beforeId: null });
    expect(planMove(docs, 'a1', 'outdent')).toEqual({ parentId: null, beforeId: 'b' });
    expect(planMove(docs, 'a', 'up')).toBeUndefined();
    expect(planMove(docs, 'a1', 'indent')).toBeUndefined();
    expect(planMove(docs, 'b', 'outdent')).toBeUndefined();
  });
});
