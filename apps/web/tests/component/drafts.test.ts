import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  adoptOrphans,
  clearDraft,
  loadDraft,
  loadOffers,
  queueDraft,
  removeOffer,
  storeDraft,
  storeOffers,
  takeOrReserve,
  withoutApplied,
} from '$lib/features/documents/drafts';
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

describe('draft queue', () => {
  it('queues drafts without overwriting or dropping any, per user', () => {
    me.userId = 'user-a';
    storeDraft('doc-4', { ...draft, title: '첫 초안' });
    expect(queueDraft('doc-4').map((d) => d.title)).toEqual(['첫 초안']);
    expect(loadDraft('doc-4')).toBeUndefined();
    // 고르는 동안 새 편집이 또 남았다가 다음에 줄 뒤에 붙는다.
    storeDraft('doc-4', { ...draft, title: '둘째 초안' });
    expect(queueDraft('doc-4').map((d) => d.title)).toEqual(['첫 초안', '둘째 초안']);
    storeOffers('doc-4', loadOffers('doc-4').slice(1));
    expect(loadOffers('doc-4').map((d) => d.title)).toEqual(['둘째 초안']);
    me.userId = 'user-b';
    expect(loadOffers('doc-4')).toEqual([]);
  });
});

describe('draft queue under a full store', () => {
  it('keeps the current draft when the queue cannot be written', () => {
    me.userId = 'user-a';
    storeDraft('doc-5', { ...draft, title: '지금 초안' });
    const setItem = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new DOMException('full', 'QuotaExceededError');
    });
    expect(queueDraft('doc-5')).toEqual([]);
    setItem.mockRestore();
    expect(loadDraft('doc-5')?.title).toBe('지금 초안');
  });

  it('removes only the resolved entry', () => {
    me.userId = 'user-a';
    const a = { ...draft, title: '가', savedAt: '2026-10-19T00:00:01Z' };
    const b = { ...draft, title: '나', savedAt: '2026-10-19T00:00:02Z' };
    storeOffers('doc-6', [a, b]);
    removeOffer('doc-6', a);
    expect(loadOffers('doc-6').map((d) => d.title)).toEqual(['나']);
  });
});

describe('draft identity', () => {
  it('treats a draft as applied only when both title and body match', () => {
    const current = { title: '회의록', body };
    const sameBodyOtherTitle = { ...draft, title: '회의록(고친 제목)' };
    const same = { ...draft, title: ' 회의록 ' };
    expect(withoutApplied([sameBodyOtherTitle, same], current).map((d) => d.title)).toEqual([
      '회의록(고친 제목)',
    ]);
  });
});

describe('drafts per tab', () => {
  it('does not let another tab of the same user overwrite or clear a draft', () => {
    me.userId = 'user-a';
    window.sessionStorage.setItem('fvoci:tab', 'tab-1');
    storeDraft('doc-7', { ...draft, title: '첫 탭' });
    window.sessionStorage.setItem('fvoci:tab', 'tab-2');
    storeDraft('doc-7', { ...draft, title: '둘째 탭' });
    clearDraft('doc-7');
    window.sessionStorage.setItem('fvoci:tab', 'tab-1');
    expect(loadDraft('doc-7')?.title).toBe('첫 탭');
  });
});

describe('orphaned tab drafts', () => {
  it('adopts only drafts of tabs that are no longer alive', async () => {
    me.userId = 'user-a';
    const prefix = 'fvoci:draft:user-a:doc-8';
    window.localStorage.setItem(
      `${prefix}:tab:tab-live`,
      JSON.stringify({ ...draft, title: '살아 있는 탭' }),
    );
    window.localStorage.setItem(
      `${prefix}:tab:tab-closed`,
      JSON.stringify({ ...draft, title: '닫힌 탭' }),
    );
    window.sessionStorage.setItem('fvoci:tab', 'tab-me');
    Object.defineProperty(navigator, 'locks', {
      configurable: true,
      value: {
        request: () => new Promise(() => undefined),
        query: () => Promise.resolve({ held: [{ name: 'fvoci:tab:tab-live' }], pending: [] }),
      },
    });
    try {
      expect(await adoptOrphans('doc-8')).toBe(1);
      expect(loadOffers('doc-8').map((d) => d.title)).toEqual(['닫힌 탭']);
      expect(window.localStorage.getItem(`${prefix}:tab:tab-live`)).not.toBeNull();
      expect(window.localStorage.getItem(`${prefix}:tab:tab-closed`)).toBeNull();
    } finally {
      Reflect.deleteProperty(navigator, 'locks');
    }
  });
});

describe('queue move failure', () => {
  it('hands the draft back to be offered when it cannot be queued', () => {
    me.userId = 'user-a';
    window.sessionStorage.setItem('fvoci:tab', 'tab-1');
    storeDraft('doc-9', { ...draft, title: '못 옮긴 초안' });
    // 이 동작 중의 쓰기는 모두 localStorage다(탭 id는 이미 있다).
    const original = Storage.prototype.setItem.bind(window.localStorage);
    const setItem = vi
      .spyOn(Storage.prototype, 'setItem')
      .mockImplementation((k: string, v: string) => {
        if (k.endsWith(':offers')) throw new DOMException('full', 'QuotaExceededError');
        original(k, v);
      });
    const taken = takeOrReserve('doc-9');
    setItem.mockRestore();
    expect(taken.reserved?.title).toBe('못 옮긴 초안');
    expect(loadDraft('doc-9')?.title).toBe('못 옮긴 초안');
  });
});
