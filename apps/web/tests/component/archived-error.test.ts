import { beforeEach, describe, expect, it } from 'vitest';

import { ApiError } from '$lib/api/client';
import { ARCHIVED_MESSAGES, errorText } from '$lib/api/errors';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

// R5 P2 F-7: 보관 거절은 서버 오류 코드가 따로 없어(409 `invalid_state`) 웹이 SQL의 고정 문구로 가린다. 문구가
// 마이그레이션에 그대로 있는지는 `server-messages.test.ts`가 본다. 여기서는 화면 문구를 본다.
describe('archived 409 messages', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
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
