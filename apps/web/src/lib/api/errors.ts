// API 오류를 화면 문구로(D-81·D-82·D-83). 서버 오류 코드는 언어와 상관없고 번역은 여기서 한다.
// 화면은 무엇을 하다 실패했는지(`action`, 예: "저장하지 못했습니다.")를 주고, 이 함수가 코드별 이유를 붙인다.
// 입력·상태 오류(400·409)의 서버 설명은 영어 진단 문구라 영어 화면에서만 그대로 보인다. 서버 쪽 실패(5xx)는
// client.ts가 이미 안전한 문구와 요청 ID로 바꿨다.
import { m } from '$lib/paraglide/messages.js';
import { getLocale } from '$lib/paraglide/runtime.js';

import { ApiError, NetworkError } from './client';

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
      if (/archived/i.test(error.message)) return m.comments_archived();
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
