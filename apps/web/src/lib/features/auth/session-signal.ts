// D-96 / SR-221-1: 탭 사이의 변경 알림일 뿐이다. 실제 사용자는 항상 /api/me로 확인한다.
const KEY = 'fvoci:auth-session';

/** 같은 사용자·로그아웃을 연달아 알려도 storage 이벤트가 나도록 매번 순번을 바꾼다. */
export function publishSessionUser(userId: string | null): void {
  try {
    localStorage.setItem(KEY, JSON.stringify({ userId, sequence: crypto.randomUUID() }));
  } catch {
    // 저장소를 막은 브라우저에서도 기존 네트워크 재확인은 계속 쓴다.
  }
}

/** 순번은 비교하지 않는다. 없는 표식·손상된 표식·접근 실패는 인증 정보로 쓰지 않는다. */
export function readSessionUser(): string | null | undefined {
  try {
    const raw = localStorage.getItem(KEY);
    const value: unknown = raw === null ? undefined : JSON.parse(raw);
    if (typeof value !== 'object' || value === null || !('userId' in value)) return undefined;
    const userId = value.userId;
    return userId === null || (typeof userId === 'string' && userId !== '') ? userId : undefined;
  } catch {
    return undefined;
  }
}

export function isSessionSignal(event: StorageEvent): boolean {
  return event.key === KEY;
}
