// D-96 / SR-221-1: 탭 사이의 변경 알림일 뿐이다. 실제 사용자는 항상 /api/me로 확인한다.
import { SESSION_SIGNAL_RECHECK_MS } from './session-check';

const KEY = 'fvoci:auth-session';
const RECHECK_KEY = `${KEY}:recheck`;
// 진행 중인 알림의 중복 게시만 잠시 줄인다. 화면 판단에는 쓰지 않는다.
const LOGOUT_PENDING_MS = SESSION_SIGNAL_RECHECK_MS;

type SessionSignal =
  { state: 'settled' } | { state: 'logout-pending'; startedAt: number; attemptId: string };

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
export async function publishSessionCheck(observedGeneration: number): Promise<boolean> {
  // 신원·해시를 두지 않는다. 시작·복원·직접 확인만 게시하고 알림으로 시작한 확인은 다시 게시하지 않는다.
  return (
    (await updateSignal(() => {
      if (readSessionGeneration() !== observedGeneration || readLogoutPendingUntil() !== undefined)
        return false;
      publish({ state: 'settled' });
      return true;
    })) ?? false
  );
}

export function publishLogoutPending(attemptId: string): void {
  publish({ state: 'logout-pending', startedAt: Date.now(), attemptId });
}

/** 요청을 끝낸 탭만 진행 표식을 최종 상태로 바꾼다. */
export async function publishLogoutResult(
  outcome: 'success' | 'failure',
  attemptId: string,
): Promise<void> {
  if (outcome === 'success') {
    publish({ state: 'settled' });
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
    publish({ state: 'settled' });
  });
}

/** 순번은 비교하지 않는다. 없는 표식·손상된 표식·접근 실패는 인증 정보로 쓰지 않는다. */
function readSignal(): StoredSignal | undefined {
  try {
    const raw = localStorage.getItem(KEY);
    const value: unknown = raw === null ? undefined : JSON.parse(raw);
    if (typeof value !== 'object' || value === null) return undefined;
    const generation =
      'generation' in value &&
      typeof value.generation === 'number' &&
      Number.isSafeInteger(value.generation) &&
      value.generation >= 0
        ? value.generation
        : 0;
    lastGeneration = Math.max(lastGeneration, generation);
    let signal: StoredSignal;
    if ('state' in value && value.state === 'logout-pending') {
      if (
        !('startedAt' in value) ||
        typeof value.startedAt !== 'number' ||
        !Number.isFinite(value.startedAt) ||
        !('attemptId' in value) ||
        typeof value.attemptId !== 'string' ||
        value.attemptId === ''
      )
        return undefined;
      signal = {
        state: 'logout-pending',
        startedAt: value.startedAt,
        attemptId: value.attemptId,
        generation,
      };
    } else if ('state' in value && value.state === 'settled') {
      signal = { state: 'settled', generation };
    } else {
      return undefined;
    }
    if ('userId' in value) {
      // SR-242-2: 이전 버전이 남긴 신원도 제거한다. 시도·기한·세대는 유지하며 신원을 읽지 않는다.
      try {
        localStorage.setItem(KEY, JSON.stringify({ ...signal, sequence: crypto.randomUUID() }));
      } catch {
        // 쓰기가 막혀도 읽은 시도로 네트워크 확인은 예약한다.
      }
    }
    return signal;
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

/** 현재 시도의 예약 정보만 읽는다. 오래된 storage 이벤트 내용으로 새 시도의 기한을 덮지 않는다. */
export function readLogoutPending(): { attemptId: string; startedAt: number } | undefined {
  const signal = readSignal();
  return signal?.state === 'logout-pending'
    ? { attemptId: signal.attemptId, startedAt: signal.startedAt }
    : undefined;
}

export function isSessionSignal(event: StorageEvent): boolean {
  return event.key === KEY || event.key === RECHECK_KEY || event.key === null;
}
