// D-96 / SR-221-1: 탭 사이의 변경 알림일 뿐이다. 실제 사용자는 항상 /api/me로 확인한다.
import { SESSION_SIGNAL_RECHECK_MS } from './session-check';

const KEY = 'fvoci:auth-session';
const RECHECK_KEY = `${KEY}:recheck`;
// 진행 중인 알림의 중복 게시만 잠시 줄인다. 화면 판단에는 쓰지 않는다.
const LOGOUT_PENDING_MS = SESSION_SIGNAL_RECHECK_MS;

type SessionSignal =
  | { state: 'settled'; userId: string | null }
  | { state: 'logout-pending'; userId: string; startedAt: number; attemptId: string };

type StoredSignal = SessionSignal & { generation: number };
let lastGeneration = 0;

/** 지원 브라우저에서는 판독·비교·갱신을 탭 사이에서 직렬화한다. 네트워크는 잠금 밖이다. */
async function updateSignal<T>(update: () => T): Promise<T | undefined> {
  try {
    if (typeof navigator !== 'undefined' && 'locks' in navigator) {
      return await navigator.locks.request(KEY, update);
    }
  } catch {
    // 비교와 쓰기를 직렬화할 수 없으면 조건부 갱신을 포기한다(SR-221-9).
  }
  return undefined;
}

function publish(signal: SessionSignal): void {
  try {
    lastGeneration = Math.max(lastGeneration, readSessionGeneration()) + 1;
    localStorage.setItem(
      KEY,
      JSON.stringify({ ...signal, generation: lastGeneration, sequence: crypto.randomUUID() }),
    );
  } catch {
    // 저장소를 막은 브라우저에서도 기존 네트워크 재확인은 계속 쓴다.
  }
}

/** 확인 결과의 중복 알림을 줄인다. 거절·잠금 대기는 자기 응답의 수락과 무관하다. */
export async function publishSessionUser(
  userId: string | null,
  observedGeneration: number,
): Promise<boolean> {
  // 새 표식 뒤의 옛 알림·같은 사용자 알림으로 탭 사이의 확인을 되풀이하지 않는다.
  return (
    (await updateSignal(() => {
      if (readSessionGeneration() !== observedGeneration || readLogoutPendingUntil() !== undefined)
        return false;
      const current = readSignal();
      if (current?.state === 'settled' && current.userId === userId) return false;
      publish({ state: 'settled', userId });
      return true;
    })) ?? false
  );
}

export function publishLogoutPending(userId: string, attemptId: string): void {
  publish({ state: 'logout-pending', userId, startedAt: Date.now(), attemptId });
}

/** 요청을 끝낸 탭만 진행 표식을 최종 상태로 바꾼다. */
export async function publishLogoutResult(userId: string | null, attemptId: string): Promise<void> {
  if (userId === null) {
    publish({ state: 'settled', userId });
    return;
  }
  // SR-221-11: 실패한 요청도 다른 탭에 재확인을 알린다. 상태 쓰기·잠금과 무관하며 새 진행 표식을 덮지 않는다.
  try {
    localStorage.setItem(RECHECK_KEY, crypto.randomUUID());
  } catch {
    // 저장소가 막혔으면 기존 창 복귀 확인을 쓴다.
  }
  await updateSignal(() => {
    const current = readSignal();
    if (current?.state !== 'logout-pending' || current.attemptId !== attemptId) return;
    publish({ state: 'settled', userId });
  });
}

/** 순번은 비교하지 않는다. 없는 표식·손상된 표식·접근 실패는 인증 정보로 쓰지 않는다. */
function readSignal(): StoredSignal | undefined {
  try {
    const raw = localStorage.getItem(KEY);
    const value: unknown = raw === null ? undefined : JSON.parse(raw);
    if (typeof value !== 'object' || value === null || !('userId' in value)) return undefined;
    const userId = value.userId;
    if (userId !== null && (typeof userId !== 'string' || userId === '')) return undefined;
    const generation =
      'generation' in value &&
      typeof value.generation === 'number' &&
      Number.isSafeInteger(value.generation) &&
      value.generation >= 0
        ? value.generation
        : 0;
    lastGeneration = Math.max(lastGeneration, generation);
    if ('state' in value && value.state === 'logout-pending') {
      if (
        userId === null ||
        !('startedAt' in value) ||
        typeof value.startedAt !== 'number' ||
        !Number.isFinite(value.startedAt) ||
        !('attemptId' in value) ||
        typeof value.attemptId !== 'string' ||
        value.attemptId === ''
      )
        return undefined;
      return {
        state: 'logout-pending',
        userId,
        startedAt: value.startedAt,
        attemptId: value.attemptId,
        generation,
      };
    }
    return { state: 'settled', userId, generation };
  } catch {
    return undefined;
  }
}

/** 게시 중복 억제용 스냅샷이다. 응답 수락·가림 판단에는 쓰지 않는다. */
export function readSessionGeneration(): number {
  return readSignal()?.generation ?? 0;
}

function readLogoutPendingUntil(): number | undefined {
  const signal = readSignal();
  if (signal?.state !== 'logout-pending') return undefined;
  const now = Date.now();
  const until = signal.startedAt + LOGOUT_PENDING_MS;
  // 시계가 뒤로 이동한 경우에도 수명이 30초보다 길어지지 않는다.
  return until > now && until <= now + LOGOUT_PENDING_MS ? until : undefined;
}

export function isSessionSignal(event: StorageEvent): boolean {
  return event.key === KEY || event.key === RECHECK_KEY || event.key === null;
}

/** 알림 내용은 지연 확인 예약에만 쓴다. 저장소를 다시 읽어 예약을 취소하거나 늦추지 않는다. */
export function isLogoutPendingSignal(event: StorageEvent): boolean {
  if (event.key !== KEY || event.newValue === null) return false;
  try {
    const value: unknown = JSON.parse(event.newValue);
    return (
      typeof value === 'object' &&
      value !== null &&
      'state' in value &&
      value.state === 'logout-pending'
    );
  } catch {
    return false;
  }
}
