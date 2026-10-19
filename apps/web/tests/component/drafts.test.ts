import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  adoptOrphans,
  clearDraft,
  loadDraft,
  loadOffers,
  queueDraft,
  removeOffer,
  storeDraft,
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
    const [first] = loadOffers('doc-4');
    if (first !== undefined) removeOffer('doc-4', first);
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
    storeDraft('doc-6', a);
    queueDraft('doc-6');
    storeDraft('doc-6', b);
    queueDraft('doc-6');
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
        if (k.includes(':offer:')) throw new DOMException('full', 'QuotaExceededError');
        original(k, v);
      });
    const taken = takeOrReserve('doc-9');
    setItem.mockRestore();
    expect(taken.reserved?.title).toBe('못 옮긴 초안');
    expect(loadDraft('doc-9')?.title).toBe('못 옮긴 초안');
  });
});

// 검토 3번: 두 탭이 거의 동시에 초안을 줄로 옮기면(세션 복원 등) 한쪽이 읽은 뒤 쓰기 전에 다른 쪽이 쓸 수 있다.
// 브라우저의 localStorage는 탭 사이에 원자적이지 않다. 어느 탭의 초안도 사라지면 안 된다.
describe('two tabs queueing at once', () => {
  it('keeps both drafts when another tab queues between this tab’s read and write', () => {
    me.userId = 'user-a';
    window.sessionStorage.setItem('fvoci:tab', 'tab-b');
    storeDraft('doc-7', { ...draft, title: 'B 탭 초안', savedAt: '2026-10-19T00:00:02Z' });
    window.sessionStorage.setItem('fvoci:tab', 'tab-a');
    storeDraft('doc-7', { ...draft, title: 'A 탭 초안', savedAt: '2026-10-19T00:00:01Z' });

    // A 탭이 처음 초안 키를 읽은 직후, 쓰기 전에 B 탭이 자기 초안을 줄로 옮긴다.
    // 탭 id(sessionStorage)도 같은 프로토타입을 쓰므로 저장소마다 원래 함수로 읽는다.
    const fromLocal = Storage.prototype.getItem.bind(window.localStorage);
    const fromSession = Storage.prototype.getItem.bind(window.sessionStorage);
    let armed = true;
    const getItem = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(function (
      this: Storage,
      k: string,
    ) {
      if (this === window.sessionStorage) return fromSession(k);
      const value = fromLocal(k);
      if (armed && k.startsWith('fvoci:draft:user-a:doc-7')) {
        armed = false;
        window.sessionStorage.setItem('fvoci:tab', 'tab-b');
        queueDraft('doc-7');
        window.sessionStorage.setItem('fvoci:tab', 'tab-a');
      }
      return value;
    });
    queueDraft('doc-7');
    getItem.mockRestore();

    expect(
      loadOffers('doc-7')
        .map((d) => d.title)
        .sort(),
    ).toEqual(['A 탭 초안', 'B 탭 초안']);
  });
});

describe('queue stored by an older version', () => {
  it('reads the old single-key list and moves it to per-draft keys', () => {
    me.userId = 'user-a';
    const old = [
      { ...draft, title: '옛 첫째', savedAt: '2026-10-19T00:00:01Z' },
      { ...draft, title: '옛 둘째', savedAt: '2026-10-19T00:00:02Z' },
    ];
    window.localStorage.setItem('fvoci:draft:user-a:doc-8:offers', JSON.stringify(old));
    expect(loadOffers('doc-8').map((d) => d.title)).toEqual(['옛 첫째', '옛 둘째']);
    expect(window.localStorage.getItem('fvoci:draft:user-a:doc-8:offers')).toBeNull();
    removeOffer('doc-8', old[0] ?? draft);
    expect(loadOffers('doc-8').map((d) => d.title)).toEqual(['옛 둘째']);
  });
});
