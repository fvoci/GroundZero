import { request, expectOk, fail } from '$lib/shared/api';
import { bool, int, optStr, rec, str, type Json } from '$lib/shared/api/parse';
import type { AuthInfo, Health, Me } from '@fvoci/api-client';

export function parseHealth(value: Json): Health {
  const o = rec(value, 'health');
  const build = rec(o.build, 'build');
  return {
    status: str(o, 'status'),
    database: str(o, 'database'),
    build: { version: str(build, 'version'), source: str(build, 'source') },
  };
}

export function parseMe(value: Json): Me {
  const o = rec(value, 'me');
  const email = o.email;
  return {
    id: str(o, 'id'),
    display_name: str(o, 'display_name'),
    email: typeof email === 'string' ? email : null,
    is_server_admin: bool(o, 'is_server_admin'),
    time_zone: typeof o.time_zone === 'string' ? o.time_zone : '',
    locale: typeof o.locale === 'string' ? o.locale : '',
    week_start: typeof o.week_start === 'string' ? o.week_start : '',
    hour_cycle: typeof o.hour_cycle === 'string' ? o.hour_cycle : '',
    theme: o.theme === 'light' || o.theme === 'dark' ? o.theme : 'system',
    last_organization_id: optStr(o, 'last_organization_id'),
    limits: parseInputLimits(rec(o.limits, 'me.limits')),
  };
}

/** 서버 입력 한도(#79). 계약상 필수이며 네 칸 모두 정수다. */
function parseInputLimits(o: Record<string, unknown>): Me['limits'] {
  return {
    max_title_chars: int(o, 'max_title_chars'),
    max_document_bytes: int(o, 'max_document_bytes'),
    max_references_per_document: int(o, 'max_references_per_document'),
    max_document_depth: int(o, 'max_document_depth'),
  };
}

export type HealthResult =
  | { kind: 'ok'; health: Health }
  | { kind: 'degraded'; health: Health }
  | { kind: 'unreachable'; message: string };

/** 상태 확인. 503도 형태가 맞으면 degraded로 본다. */
export async function fetchHealth(fetchImpl: typeof fetch = fetch): Promise<HealthResult> {
  let response: Response;
  try {
    response = await fetchImpl('/healthz');
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message : 'network error';
    return { kind: 'unreachable', message };
  }
  const body: unknown = await response.json().catch(() => undefined);
  let health: Health;
  try {
    health = parseHealth(body);
  } catch {
    return { kind: 'unreachable', message: `unexpected response (${String(response.status)})` };
  }
  return response.ok ? { kind: 'ok', health } : { kind: 'degraded', health };
}

export const accountsApi = {
  authInfo: () =>
    expectOk('GET', '/api/auth-info', (v): AuthInfo => ({
      oidc_enabled: bool(rec(v, 'auth info'), 'oidc_enabled'),
    })),
  /** 로그인하지 않았으면 undefined. */
  async me(this: void): Promise<Me | undefined> {
    const { status, json } = await request('GET', '/api/me');
    if (status === 401) return undefined;
    if (status !== 200) fail(status, json);
    return parseMe(json);
  },
  async logout(this: void): Promise<string> {
    return expectOk('POST', '/auth/logout', (v) => str(rec(v, 'logout'), 'redirect_to'));
  },
};
