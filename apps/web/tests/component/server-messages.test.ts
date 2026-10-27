import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';

import { ApiError } from '$lib/api/client';
import {
  ARCHIVED_MESSAGES,
  MATCHED_SERVER_MESSAGES,
  STATUS_NAME_EXISTS_MESSAGE,
  UNTIL_BEFORE_START_MESSAGE,
  isArchivedRejection,
  statusNameExists,
  untilBeforeStart,
} from '$lib/api/errors';

// 계약에 따로 오류 코드가 없는 거절을 웹은 서버 본문의 고정 문구로 판별한다(보관 409 세 문구 R5 P2 F-7, 반복 끝 400
// R5 F-6, 상태 이름 중복 409 R5 delta F-2). 문구는 마이그레이션 SQL의 RAISE EXCEPTION에서 오고 서버 시험은 문구를
// 고정하지 않으므로, 서버가 문구나 SQLSTATE를 바꾸면 웹 판별이 조용히 일반 안내로 떨어지지 않게 여기서 한꺼번에 본다.
// 판별 문구는 모두 `src/lib/api/errors.ts`의 `MATCHED_SERVER_MESSAGES`에 둔다.
const migrations = path.resolve(import.meta.dirname, '../../../../crates/server/migrations');
const sql = readdirSync(migrations)
  .filter((name) => name.endsWith('.sql'))
  .map((name) => readFileSync(path.join(migrations, name), 'utf8'))
  .join('\n');

describe('server messages the web matches', () => {
  it.each(MATCHED_SERVER_MESSAGES)(
    'finds "$message" raised with SQLSTATE $sqlstate in the migrations',
    ({ message, sqlstate }) => {
      expect(sql).toContain(`RAISE EXCEPTION '${message}' USING ERRCODE = '${sqlstate}'`);
    },
  );

  it('lists every message a predicate matches', () => {
    expect(MATCHED_SERVER_MESSAGES.map((entry) => entry.message).sort()).toEqual(
      [...ARCHIVED_MESSAGES, STATUS_NAME_EXISTS_MESSAGE, UNTIL_BEFORE_START_MESSAGE].sort(),
    );
  });

  it('covers every archived 55000 error the migrations raise', () => {
    const raised = new Set(
      [...sql.matchAll(/RAISE EXCEPTION '([^']*archived[^']*)' USING ERRCODE = '55000'/g)].map(
        (match) => match[1],
      ),
    );
    expect([...raised].sort()).toEqual([...ARCHIVED_MESSAGES].sort());
  });

  it('matches each message only with the status code the server maps its SQLSTATE to', () => {
    for (const message of ARCHIVED_MESSAGES) {
      expect(isArchivedRejection(new ApiError(409, 'invalid_state', message))).toBe(true);
      expect(isArchivedRejection(new ApiError(400, 'bad_request', message))).toBe(false);
    }
    expect(statusNameExists(new ApiError(409, 'invalid_state', STATUS_NAME_EXISTS_MESSAGE))).toBe(
      true,
    );
    expect(statusNameExists(new ApiError(409, 'invalid_state', 'other'))).toBe(false);
    expect(statusNameExists(new Error(STATUS_NAME_EXISTS_MESSAGE))).toBe(false);
    expect(untilBeforeStart(new ApiError(400, 'bad_request', UNTIL_BEFORE_START_MESSAGE))).toBe(
      true,
    );
    expect(untilBeforeStart(new ApiError(400, 'bad_request', 'other'))).toBe(false);
    expect(untilBeforeStart(new Error(UNTIL_BEFORE_START_MESSAGE))).toBe(false);
  });
});
