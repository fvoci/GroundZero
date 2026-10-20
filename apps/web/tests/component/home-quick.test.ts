import { describe, expect, it } from 'vitest';

import { RetryKey } from '$lib/api/retry-key';
import { quickSend } from '$lib/features/home/quick';

// 홈 빠른 추가의 재시도 키는 실제로 보내는 값으로 만든다. 보내지 않는 칸(메모의 대상·날짜)을 바꿔도 같은 요청이고,
// 날짜를 비운 일정은 오늘로 보내므로 오늘을 직접 고른 것과 같은 요청이다.
describe('home quick add request', () => {
  it('keys a memo by its title only', () => {
    const key = new RetryKey();
    const first = quickSend({ what: 'memo', title: ' 메모 ', item: 'i1', date: '' }, '2026-10-20');
    const again = quickSend(
      { what: 'memo', title: '메모', item: 'i2', date: '2026-10-22' },
      '2026-10-20',
    );
    expect(key.for(first)).toBe(key.for(again));
  });

  it('sends an event with an empty date on today and keys it the same', () => {
    const key = new RetryKey();
    const empty = quickSend({ what: 'event', title: '회의', item: 'i1', date: '' }, '2026-10-20');
    expect(empty).toEqual({ what: 'event', item: 'i1', title: '회의', date: '2026-10-20' });
    const today = quickSend(
      { what: 'event', title: '회의', item: 'i1', date: '2026-10-20' },
      '2026-10-20',
    );
    expect(key.for(empty)).toBe(key.for(today));
    const tomorrow = quickSend(
      { what: 'event', title: '회의', item: 'i1', date: '2026-10-21' },
      '2026-10-20',
    );
    expect(key.for(tomorrow)).not.toBe(key.for(today));
  });

  it('needs an item for tasks and events', () => {
    expect(quickSend({ what: 'task', title: '할 일', item: '', date: '' }, '2026-10-20')).toBe(
      undefined,
    );
  });
});
