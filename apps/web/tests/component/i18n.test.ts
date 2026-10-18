import { describe, expect, it } from 'vitest';

import { m } from '$lib/paraglide/messages.js';

describe('translated messages (D-81)', () => {
  it('substitutes variables and picks plural forms per language', () => {
    expect(m.common_items_count({ count: 1 }, { locale: 'en' })).toBe('1 item');
    expect(m.common_items_count({ count: 3 }, { locale: 'en' })).toBe('3 items');
    expect(m.common_items_count({ count: 3 }, { locale: 'ko' })).toBe('3개');
    expect(m.app_wiki_of({ name: '연구실' }, { locale: 'en' })).toBe('연구실 wiki');
    expect(m.events_day_today({ day: '2026-10-17' }, { locale: 'ko' })).toBe('2026-10-17 (오늘)');
  });

  it('keeps user-written names untouched', () => {
    // 사용자가 쓴 이름은 번역하지 않고 그대로 끼워 넣는다.
    expect(m.events_entry_all_day({ title: 'Lab meeting 회의' }, { locale: 'ko' })).toBe(
      '종일 Lab meeting 회의',
    );
  });
});
