import { untrack } from 'svelte';
import {
  auth,
  accountSafety,
  connection,
  reportConnectionLost,
  reportSessionExpired,
  sessionExpiries,
  switchAccount,
} from '$lib/features/feedback/feedback.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { isRecord, type Json } from './parse';
export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
    /** 서버가 붙인 요청 id(D-83). 운영 로그에서 같은 요청을 찾을 때 쓴다. */
    readonly requestId?: string,
    readonly reason?: string,
    readonly field?: string,
    readonly limit?: number,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

/** 네트워크 실패(응답 없음). 같은 request_id로 재시도할 수 있다. */
export class NetworkError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'NetworkError';
  }
}

export const EXPECTED_USER = 'x-fvoci-expected-user';

function blockedBySafety(method: string, path: string): boolean {
  return untrack(() => {
    if (accountSafety.switching) {
      // #212 1·2행, 코디네이터 추가 지시: 확인 전에는 이 두 정확한 제어 요청만 허용한다.
      return !(
        (accountSafety.reason === 'logout' && method === 'POST' && path === '/auth/logout') ||
        (accountSafety.reason === 'restore' && method === 'GET' && path === '/api/me')
      );
    }
    // 같은 사용자임을 확인한 뒤 화면을 새로 읽는 동안에도 쓰기는 열지 않는다.
    return accountSafety.restoring && method !== 'GET';
  });
}

export async function request(
  method: string,
  path: string,
  body?: Json,
  extra: Record<string, string> = {},
  // CSV 가져오기 취소 신호를 fetch까지 전달한다.
  signal?: AbortSignal,
): Promise<{ status: number; json: Json; etag: string | null }> {
  // 계정 변경 감지 뒤 자동 재시도·늦은 응답을 기존 화면에 적용하지 않는다(D-96).
  // 전환 상태는 반응형 읽기가 아니다: 화면의 $effect가 요청을 부를 때 여기서 읽은 switching이 그 효과의 의존성이 되면, 전환이
  // 시작되는 순간 그 효과들이 다시 실행돼 화면이 로딩 상태로 돌아가고(DOM·메모리 입력·떠나기 가드가 사라짐) 입력 보존 설계가
  // 깨진다(#88). 그래서 untrack으로 읽는다. 전환 중 요청을 막는 동작은 같다.
  const generation = untrack(() => accountSafety.generation);
  if (blockedBySafety(method, path))
    throw new ApiError(409, 'account_changed', m.common_error_account_changed());
  let response: Response;
  const expiriesAtStart = sessionExpiries();
  try {
    const headers = new Headers(extra);
    if (method !== 'GET' && method !== 'HEAD' && me.userId !== '' && !headers.has(EXPECTED_USER)) {
      headers.set(EXPECTED_USER, me.userId);
    }
    if (body !== undefined) headers.set('Content-Type', 'application/json');
    const init: RequestInit = {
      method,
      credentials: 'same-origin',
      headers,
    };
    if (body !== undefined) init.body = JSON.stringify(body);
    if (signal !== undefined) init.signal = signal;
    response = await fetch(path, init);
  } catch (error: unknown) {
    if (signal?.aborted === true) throw error;
    // 연결 배너(D-82). 각 화면은 자기 자리에서 입력을 지키고 다시 시도한다.
    reportConnectionLost();
    throw new NetworkError(error instanceof Error ? error.message : 'network error');
  }
  const text = await response.text();
  // 진행 중이던 요청도 계정 전환 안전 상태를 벗어나지 못한다.
  if (generation !== untrack(() => accountSafety.generation) || blockedBySafety(method, path))
    throw new ApiError(409, 'account_changed', m.common_error_account_changed());
  let json: Json = null;
  if (text !== '') {
    try {
      json = JSON.parse(text);
    } catch {
      json = null;
    }
  }
  // 앞단 프록시가 대신 답한 게이트웨이 오류(앱의 JSON 오류 형식이 아님)는 서버에 닿지 않은 것으로 본다(D-82 배너).
  const gateway =
    (response.status === 502 || response.status === 503 || response.status === 504) &&
    !(isRecord(json) && typeof json.error === 'string');
  if (gateway) reportConnectionLost();
  else if (connection.lost) connection.lost = false;
  // 세션이 필요한 요청이 다시 성공하면(다른 탭에서 로그인함) 로그인 배너를 내린다. 마지막 401보다 먼저 시작한
  // 요청의 늦은 성공은 세지 않는다.
  if (auth.expired && response.ok && needsSession(path) && expiriesAtStart === sessionExpiries())
    auth.expired = false;
  return { status: response.status, json, etag: response.headers.get('ETag') };
}

/** 로그인 없이도 답하는 경로(상태 확인·로그인 방식·공개 링크)가 아닌 API. */
function needsSession(path: string): boolean {
  return path.startsWith('/api/') && path !== '/api/auth-info' && !path.startsWith('/api/public/');
}

export function fail(status: number, json: Json): never {
  // 쓰는 중에 세션이 끝났다(로그인 확인 `api.me()`의 401은 여기 오지 않는다). 앱 전체 배너로 새 탭 로그인을 권한다.
  if (status === 401) reportSessionExpired();
  // 이 창의 편집을 시작한 계정이 아닌 계정의 세션으로 보냈다(다른 탭에서 계정을 바꿈). 서버는 아무것도 바꾸지
  // 않았다. 이 창은 앞 계정의 것이라 남기지 않고 계정 전환 처리(다시 읽기)로 넘긴다. 화면은 다시 보내지 않는다.
  if (status === 409 && isRecord(json) && json.error === 'account_changed') switchAccount();
  if (isRecord(json) && typeof json.error === 'string') {
    let message = typeof json.message === 'string' ? json.message : json.error;
    const requestId = typeof json.request_id === 'string' ? json.request_id : undefined;
    // 서버 쪽 실패(5xx)는 안전한 문구와 요청 id만 보인다(D-83). 바깥 서비스 오류의 설명은 서버가 사람용으로 준 것이다.
    if (status >= 500 && requestId !== undefined) {
      message =
        json.error === 'upstream_error'
          ? `${message} (${m.common_request_id({ requestId })})`
          : m.common_server_error({ requestId });
    }
    throw new ApiError(
      status,
      json.error,
      message,
      requestId,
      typeof json.reason === 'string' ? json.reason : undefined,
      typeof json.field === 'string' ? json.field : undefined,
      typeof json.limit === 'number' ? json.limit : undefined,
    );
  }
  // 앱의 오류 본문이 아닌 응답: 앞단 프록시의 게이트웨이 오류(서버에 닿지 않음)거나 예상하지 못한 응답이다.
  // 화면에는 번역된 이유를 보인다(D-81·D-82).
  if (status === 502 || status === 503 || status === 504) {
    throw new ApiError(status, 'unreachable', m.common_error_network());
  }
  throw new ApiError(status, 'unexpected', m.common_error_unexpected({ status: String(status) }));
}

export async function expectOk<T>(
  method: string,
  path: string,
  parse: (v: Json) => T,
  body?: Json,
  // 선택적 취소 신호(기존 호출은 그대로).
  signal?: AbortSignal,
): Promise<T> {
  const { status, json } = await request(method, path, body, {}, signal);
  if (status < 200 || status >= 300) fail(status, json);
  return parse(json);
}
