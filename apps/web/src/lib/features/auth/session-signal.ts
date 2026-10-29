// D-96 / SR-221-1: 탭 사이의 변경 알림일 뿐이다. 실제 사용자는 항상 /api/me로 확인한다.
const KEY = 'fvoci:auth-session';
// 끝을 알리지 못하고 닫힌 탭이 다른 탭을 영구히 가리지 않게 한다(SR-221-4).
const LOGOUT_PENDING_MS = 30_000;
/** 안전 세대 변경 뒤 무한 확인을 피한다. 첫 요청과 재확인 두 번이다(SR-221-8). */
export const SESSION_CHECK_MAX_ATTEMPTS = 3;

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

async function publishHidingSignal(signal: SessionSignal): Promise<void> {
  const published = await updateSignal(() => {
    publish(signal);
    return true;
  });
  if (published !== true) publish(signal);
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

/** 같은 사용자·로그아웃을 연달아 알려도 storage 이벤트가 나도록 매번 순번을 바꾼다. */
export async function publishSessionUser(
  userId: string | null,
  observedGeneration: number,
): Promise<boolean> {
  // 로그아웃 전의 /api/me는 최종 상태가 아니다. 셸 시작·창 복귀·복원 모두 같은 경계를 쓴다.
  return (
    (await updateSignal(() => {
      if (readSessionGeneration() !== observedGeneration || readLogoutPendingUntil() !== undefined)
        return false;
      publish({ state: 'settled', userId });
      return true;
    })) ?? false
  );
}

export async function publishLogoutPending(userId: string, attemptId: string): Promise<void> {
  await publishHidingSignal({ state: 'logout-pending', userId, startedAt: Date.now(), attemptId });
}

/** 요청을 끝낸 탭만 진행 표식을 최종 상태로 바꾼다. */
export async function publishLogoutResult(userId: string | null, attemptId: string): Promise<void> {
  if (userId === null) {
    await publishHidingSignal({ state: 'settled', userId });
    return;
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

/** /api/me 요청 직전에 읽고, 응답을 게시할 때 같은 세대인지 검사한다(SR-221-5). */
export function readSessionGeneration(): number {
  return readSignal()?.generation ?? 0;
}

/** 진행 표식은 사용자 일치로 무시하지 않는다. 만료 뒤에도 익명 신호로 일반 확인을 유도한다. */
export function readSessionUser(): string | null | undefined {
  const signal = readSignal();
  return signal?.state === 'logout-pending' ? null : signal?.userId;
}

export function readLogoutPendingUntil(): number | undefined {
  const signal = readSignal();
  if (signal?.state !== 'logout-pending') return undefined;
  const now = Date.now();
  const until = signal.startedAt + LOGOUT_PENDING_MS;
  // 시계가 뒤로 이동한 경우에도 수명이 30초보다 길어지지 않는다.
  return until > now && until <= now + LOGOUT_PENDING_MS ? until : undefined;
}

export function isSessionSignal(event: StorageEvent): boolean {
  return event.key === KEY || event.key === null;
}
