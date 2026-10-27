import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

import { ApiError } from '$lib/api/client';
import {
  UNTIL_BEFORE_START_MESSAGE,
  untilBeforeStart,
} from '$lib/features/events/RecurrenceEditor.svelte';

// F-6(R5 B): 반복 끝 거절은 계약의 오류 코드가 없어 웹이 서버 400 본문의 문구로 판별한다. 그 문구는 서버 마이그레이션 SQL의
// RAISE EXCEPTION에서 온다. 서버 시험은 문구를 고정하지 않으므로, 서버가 문구를 바꾸면 웹 판별이 조용히 일반 오류로
// 떨어지지 않게 여기서 기준 SQL에 웹이 견주는 문구가 그대로 있는지 확인한다.
const migrations = resolve(import.meta.dirname, '../../../../crates/server/migrations');

describe('recurrence until rejection message', () => {
  it('is still raised verbatim by the server migrations', () => {
    const raised = readdirSync(migrations)
      .filter((name) => name.endsWith('.sql'))
      .flatMap((name) =>
        readFileSync(resolve(migrations, name), 'utf8')
          .split('\n')
          .filter((line) => line.includes(`RAISE EXCEPTION '${UNTIL_BEFORE_START_MESSAGE}'`)),
      );
    expect(raised.length).toBeGreaterThan(0);
  });

  it('matches only the server bad request with that message', () => {
    expect(untilBeforeStart(new ApiError(400, 'bad_request', UNTIL_BEFORE_START_MESSAGE))).toBe(
      true,
    );
    expect(untilBeforeStart(new ApiError(400, 'bad_request', 'other'))).toBe(false);
    expect(untilBeforeStart(new Error(UNTIL_BEFORE_START_MESSAGE))).toBe(false);
  });
});
