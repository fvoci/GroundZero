// 브라우저 저장소 키(D-121): 사용자와 조직을 넣어 다른 계정·다른 조직의 기억(마지막 워크스페이스, 보드 보기 등)이
// 섞이지 않게 한다. 값은 이 브라우저만의 편의 기억이고 정본이 아니다. 저장소를 못 쓰는 환경(비공개 창 등)은 조용히 넘긴다.

/** `fvoci:<user>:<organization>:<name>`. 조직 밖 기억(조직 선택 자체)은 `organization`을 `-`로 둔다. */
export function scopedKey(
  name: string,
  userId: string,
  organizationId: string | undefined,
): string {
  return `fvoci:${userId}:${organizationId ?? '-'}:${name}`;
}

export function readLocal(key: string): string | null {
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function writeLocal(key: string, value: string | null): void {
  try {
    if (value === null) window.localStorage.removeItem(key);
    else window.localStorage.setItem(key, value);
  } catch {
    // 저장소를 못 쓰면 기억하지 않는다.
  }
}

export function readSession(key: string): string | null {
  try {
    return window.sessionStorage.getItem(key);
  } catch {
    return null;
  }
}

export function writeSession(key: string, value: string | null): void {
  try {
    if (value === null) window.sessionStorage.removeItem(key);
    else window.sessionStorage.setItem(key, value);
  } catch {
    // 저장소를 못 쓰면 기억하지 않는다.
  }
}
