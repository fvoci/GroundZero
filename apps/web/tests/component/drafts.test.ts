import { afterEach, describe, expect, it } from 'vitest';

import { clearDraft, loadDraft, storeDraft } from '$lib/features/documents/drafts';
import { me } from '$lib/me.svelte';

const body = { type: 'doc', content: [{ type: 'paragraph' }] };
const draft = { baseRevision: 3, title: 'A의 초안', body, savedAt: '2026-10-19T00:00:00Z' };

afterEach(() => {
  window.localStorage.clear();
  me.userId = '';
});

// 같은 브라우저를 함께 쓰는 사용자 사이에서 초안이 섞이지 않는다(제품 로직 점검 3번 불변식).
describe('drafts are per user', () => {
  it('does not offer one user’s draft to another', () => {
    me.userId = 'user-a';
    storeDraft('doc-1', draft);
    expect(loadDraft('doc-1')?.title).toBe('A의 초안');

    me.userId = 'user-b';
    expect(loadDraft('doc-1')).toBeUndefined();
    clearDraft('doc-1');

    me.userId = 'user-a';
    expect(loadDraft('doc-1')?.title).toBe('A의 초안');
  });

  it('drops drafts stored without a user and keeps nothing before login', () => {
    window.localStorage.setItem('fvoci:draft:doc-2', JSON.stringify(draft));
    me.userId = 'user-b';
    expect(loadDraft('doc-2')).toBeUndefined();
    expect(window.localStorage.getItem('fvoci:draft:doc-2')).toBeNull();

    me.userId = '';
    storeDraft('doc-3', draft);
    expect(window.localStorage.length).toBe(0);
  });
});

describe('draft slots', () => {
  it('keeps the offered draft and new edits apart, per user', () => {
    me.userId = 'user-a';
    storeDraft('doc-4', { ...draft, title: '고를 초안' }, 'offer');
    storeDraft('doc-4', { ...draft, title: '새 편집' });
    expect(loadDraft('doc-4', 'offer')?.title).toBe('고를 초안');
    expect(loadDraft('doc-4')?.title).toBe('새 편집');
    clearDraft('doc-4', 'offer');
    expect(loadDraft('doc-4', 'offer')).toBeUndefined();
    expect(loadDraft('doc-4')?.title).toBe('새 편집');
    me.userId = 'user-b';
    expect(loadDraft('doc-4')).toBeUndefined();
  });
});
