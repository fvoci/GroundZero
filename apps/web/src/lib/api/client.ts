// 생성된 계약 타입을 쓰는 API 클라이언트. 생성 타입은 런타임 검증을 대신하지 않으므로
// 응답은 unknown으로 받아 실제 형태를 확인한다 (docs/development.md 3절).
import type { Health } from '@fvoci/api-client';

export type HealthResult =
  | { kind: 'ok'; health: Health }
  | { kind: 'degraded'; health: Health }
  | { kind: 'unreachable'; message: string };

function parseHealth(value: unknown): Health | undefined {
  if (typeof value !== 'object' || value === null) return undefined;
  if (!('status' in value) || !('database' in value)) return undefined;
  const { status, database } = value;
  if (typeof status !== 'string' || typeof database !== 'string') return undefined;
  return { status, database };
}

export async function fetchHealth(fetchImpl: typeof fetch = fetch): Promise<HealthResult> {
  let response: Response;
  try {
    response = await fetchImpl('/healthz');
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message : 'network error';
    return { kind: 'unreachable', message };
  }
  const body: unknown = await response.json().catch(() => undefined);
  const health = parseHealth(body);
  if (health === undefined) {
    return { kind: 'unreachable', message: `unexpected response (${String(response.status)})` };
  }
  return response.ok ? { kind: 'ok', health } : { kind: 'degraded', health };
}
