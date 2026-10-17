import { describe, expect, it } from 'vitest';

import type { SearchResultView } from '$lib/api/client';
import { searchResultPath } from '$lib/features/search/search';

const W = '11111111-1111-1111-1111-111111111111';
const I = '22222222-2222-2222-2222-222222222222';
const X = '33333333-3333-3333-3333-333333333333';

const result = (over: Partial<SearchResultView>): SearchResultView => ({
  kind: 'document',
  id: X,
  title: 't',
  reference: null,
  workspace_id: W,
  item_id: null,
  document_id: null,
  snippet: null,
  ...over,
});

const member = (ws: string): boolean => ws === W;

describe('search result paths', () => {
  it('opens each kind where it lives', () => {
    expect(searchResultPath(result({}), member)).toBe(`/w/${W}/wiki/${X}`);
    expect(searchResultPath(result({ item_id: I }), member)).toBe(`/w/${W}/i/${I}/d/${X}`);
    expect(searchResultPath(result({ kind: 'task', item_id: I }), member)).toBe(
      `/w/${W}/i/${I}/t/${X}`,
    );
    expect(searchResultPath(result({ kind: 'item' }), member)).toBe(`/w/${W}/i/${X}`);
    expect(searchResultPath(result({ kind: 'event', item_id: I }), member)).toBe(`/w/${W}/i/${I}`);
    expect(searchResultPath(result({ kind: 'file', document_id: X, id: I }), member)).toBe(
      `/w/${W}/wiki/${X}`,
    );
    expect(searchResultPath(result({ kind: 'file', item_id: I }), member)).toBe(`/w/${W}/i/${I}`);
  });

  it('opens notes and documents shared from outside as standalone', () => {
    expect(searchResultPath(result({ workspace_id: null }), member)).toBe(`/d/${X}`);
    expect(searchResultPath(result({ workspace_id: I }), member)).toBe(`/d/${X}`);
  });
});
