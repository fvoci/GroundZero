// API 오류를 화면 문구로(D-81·D-82·D-83). 서버 오류 코드는 언어와 상관없고 번역은 여기서 한다.
// 화면은 무엇을 하다 실패했는지(`action`, 예: "저장하지 못했습니다.")를 주고, 이 함수가 코드별 이유를 붙인다.
// 입력·상태 오류(400·409)의 서버 설명은 영어 진단 문구라 영어 화면에서만 그대로 보인다. 서버 쪽 실패(5xx)는
// client.ts가 이미 안전한 문구와 요청 ID로 바꿨다.
import { m } from '$lib/paraglide/messages.js';
import { getLocale } from '$lib/paraglide/runtime.js';

import { ApiError, NetworkError } from './client';

// 계약에 따로 오류 코드가 없는 거절은 서버 본문의 고정 문구(마이그레이션 SQL의 RAISE EXCEPTION)로 가린다. 그 문구는 모두
// 여기에 두고 `MATCHED_SERVER_MESSAGES`에 SQLSTATE와 함께 적는다. `tests/component/server-messages.test.ts`가 마이그레이션에
// 문구와 SQLSTATE가 그대로 있는지 본다.

/** 보관된 항목·모듈에 대한 거절(SQL 55000 → 409 `invalid_state`). */
export const ARCHIVED_MESSAGES: readonly string[] = [
  'the item is archived',
  'the module is archived',
  'the target item is archived',
];

/** 같은 이름의 업무 상태가 이미 있다(SQL 55000 → 409 `invalid_state`). */
export const STATUS_NAME_EXISTS_MESSAGE = 'a status with this name exists';

/** 반복 끝이 첫 회차보다 앞선다(SQL 22023 → 400 `bad_request`, 일정·업무 반복 공통). */
export const UNTIL_BEFORE_START_MESSAGE = 'until must not be before the first occurrence';

/** 웹이 판별하는 서버 문구와 그 SQLSTATE 전부. */
export const MATCHED_SERVER_MESSAGES: readonly { message: string; sqlstate: string }[] = [
  ...ARCHIVED_MESSAGES.map((message) => ({ message, sqlstate: '55000' })),
  { message: STATUS_NAME_EXISTS_MESSAGE, sqlstate: '55000' },
  { message: UNTIL_BEFORE_START_MESSAGE, sqlstate: '22023' },
];

/** 보관 때문에 거절된 쓰기인지. */
export function isArchivedRejection(error: unknown): boolean {
  return (
    error instanceof ApiError &&
    error.code === 'invalid_state' &&
    ARCHIVED_MESSAGES.includes(error.message)
  );
}

/** 상태 이름이 겹쳐 거절됐는지. */
export function statusNameExists(error: unknown): boolean {
  return (
    error instanceof ApiError &&
    error.code === 'invalid_state' &&
    error.message === STATUS_NAME_EXISTS_MESSAGE
  );
}

/** 반복 끝을 시작일보다 앞에 두어 서버가 거절했는지. */
export function untilBeforeStart(error: unknown): boolean {
  return (
    error instanceof ApiError &&
    error.code === 'bad_request' &&
    error.message === UNTIL_BEFORE_START_MESSAGE
  );
}

function reason(error: ApiError): string {
  switch (error.code) {
    case 'unauthorized':
      return m.common_error_unauthorized();
    case 'forbidden':
    case 'bad_origin':
      return m.common_error_forbidden();
    case 'not_found':
      return m.common_error_not_found();
    case 'bad_request':
      return getLocale() === 'en' ? error.message : m.common_error_bad_request();
    case 'invalid_state':
      // 보관 상태의 409는 새로고침 안내로 숨기지 않는다(D-20·D-106).
      if (isArchivedRejection(error)) return m.common_error_archived();
      return getLocale() === 'en' ? error.message : m.common_error_invalid_state();
    case 'account_changed':
      return m.common_error_account_changed();
    case 'too_large':
      return m.common_error_too_large();
    case 'unexpected':
      // 앱의 오류 본문이 아닌 응답(예: 앞단 프록시의 HTML 413): client.ts가 번역한 문구와 상태.
      return error.message;
    default:
      return '';
  }
}

/** 실패한 행동과 이유를 한 문구로. 알 수 없는 오류는 행동만. */
export function errorText(error: unknown, action: string): string {
  if (error instanceof NetworkError) return `${action} ${m.common_error_network()}`;
  if (!(error instanceof ApiError)) return action;
  // 서버에 닿지 않은 경우는 무엇을 하다 실패했는지와 함께, 그 밖의 서버 쪽 실패는 client.ts가 만든 안전한 문구 그대로.
  if (error.code === 'unreachable') return `${action} ${error.message}`;
  if (error.status >= 500) return error.message;
  const why = reason(error);
  return why === '' ? action : `${action} ${why}`;
}
