import { workApi as work } from '$lib/features/tasks/api';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ApiError } from '$lib/api/client';

// 관계 설정의 409는 두 모양이다(D-91, 계약 `PredecessorConflict`): 승인 목록이 맞지 않으면 TaskConflict(최신본 포함),
// 보관·동시 변경·id 부족이면 ErrorBody(`invalid_state`, 최신본 없음). 클라이언트는 앞의 것만 다시 묻기로 돌리고 뒤의
// 것은 일반 오류로 알린다(최신본이 있다고 가정하지 않는다).
const task = { predecessor_id: 'a', successor_id: 'b', kind: 'FS', lag_days: 0 };

function reply(status: number, json: object): void {
  vi.stubGlobal('fetch', () =>
    Promise.resolve(
      new Response(JSON.stringify(json), {
        status,
        headers: { 'Content-Type': 'application/json' },
      }),
    ),
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('link conflict responses', () => {
  it('turns a stale approval (TaskConflict) into a shift conflict with the latest task', async () => {
    reply(409, { error: 'shift_conflict', current: task });
    const result = await work.setPredecessor('user-a', 'b', 'a', 'SS', 1, null, []);
    expect(result.kind).toBe('shift_conflict');
    expect(result.kind === 'shift_conflict' ? result.current?.successor_id : '').toBe('b');
  });

  it('turns a changed link (TaskConflict link_conflict) into a link conflict with the latest task', async () => {
    reply(409, { error: 'link_conflict', current: task });
    const result = await work.setPredecessor(
      'user-a',
      'b',
      'a',
      'SS',
      1,
      { kind: 'FS', lag_days: 0 },
      [],
    );
    expect(result.kind).toBe('link_conflict');
    expect(result.kind === 'link_conflict' ? result.current?.successor_id : '').toBe('b');
  });

  it('reports an archived or retried change (ErrorBody) as an ordinary 409 error', async () => {
    reply(409, { error: 'invalid_state', message: 'the item is archived', request_id: 'r1' });
    const failed = await work.setPredecessor('user-a', 'b', 'a', 'SS', 1, null, []).then(
      () => undefined,
      (e: unknown) => e,
    );
    expect(failed).toBeInstanceOf(ApiError);
    expect(failed instanceof ApiError ? [failed.status, failed.code] : []).toEqual([
      409,
      'invalid_state',
    ]);
  });
});
