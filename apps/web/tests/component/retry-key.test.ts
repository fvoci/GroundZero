import { describe, expect, it } from 'vitest';

import { RetryKey } from '$lib/api/retry-key';

// 응답을 잃고 다시 보낼 때: 같은 내용이면 같은 request_id(중복 없음), 고친 내용이면 새 id(서버가 처음 것을
// 돌려줘 고친 내용이 사라지지 않게). 제품 로직 점검 2번 불변식.
describe('RetryKey', () => {
  it('keeps the id for the same payload and rotates it when the payload changes', () => {
    const key = new RetryKey();
    const first = key.for({ body: '처음 댓글', target: 'doc-1' });
    expect(key.for({ target: 'doc-1', body: '처음 댓글' })).toBe(first);
    const edited = key.for({ body: '고친 댓글', target: 'doc-1' });
    expect(edited).not.toBe(first);
    expect(key.for({ body: '고친 댓글', target: 'doc-1' })).toBe(edited);
    key.done();
    expect(key.for({ body: '고친 댓글', target: 'doc-1' })).not.toBe(edited);
  });
});
