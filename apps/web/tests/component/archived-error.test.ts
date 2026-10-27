import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { beforeEach, describe, expect, it } from 'vitest';

import { ApiError } from '$lib/api/client';
import { ARCHIVED_MESSAGES, errorText } from '$lib/api/errors';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

// R5 P2 F-7: 보관 거절은 서버 오류 코드가 따로 없어(409 `invalid_state`) 웹이 SQL의 고정 문구로 가린다. 서버 시험은 이
// 문구를 고정하지 않으므로, 마이그레이션에서 문구가 바뀌거나 사라지면 여기서 드러나게 한다.
const migrations = path.resolve(import.meta.dirname, '../../../../crates/server/migrations');
const sql = readdirSync(migrations)
  .filter((name) => name.endsWith('.sql'))
  .map((name) => readFileSync(path.join(migrations, name), 'utf8'))
  .join('\n');

describe('archived 409 messages', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });

  it.each(ARCHIVED_MESSAGES)('finds "%s" as a 55000 error in the migrations', (message) => {
    expect(sql).toContain(`RAISE EXCEPTION '${message}' USING ERRCODE = '55000'`);
  });

  it('covers every archived 55000 error the migrations raise', () => {
    const raised = new Set(
      [...sql.matchAll(/RAISE EXCEPTION '([^']*archived[^']*)' USING ERRCODE = '55000'/g)].map(
        (match) => match[1],
      ),
    );
    expect([...raised].sort()).toEqual([...ARCHIVED_MESSAGES].sort());
  });

  it.each(ARCHIVED_MESSAGES)('shows the shared archived reason for "%s"', (message) => {
    expect(errorText(new ApiError(409, 'invalid_state', message), '저장하지 못했습니다.')).toBe(
      `저장하지 못했습니다. ${m.common_error_archived()}`,
    );
  });

  it('keeps other invalid_state conflicts on the reload hint', () => {
    expect(
      errorText(
        new ApiError(409, 'invalid_state', 'a status with this name exists'),
        '저장하지 못했습니다.',
      ),
    ).toBe(`저장하지 못했습니다. ${m.common_error_invalid_state()}`);
  });
});
