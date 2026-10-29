/** 안전 세대 변경 뒤 무한 확인을 피한다. 첫 요청과 재확인 두 번이다(SR-221-8). */
export const SESSION_CHECK_MAX_ATTEMPTS = 3;

/** 최종 알림 없이 닫힌 로그아웃 탭도 한 번 더 확인한다(SR-221-12). */
export const SESSION_SIGNAL_RECHECK_MS = 30_000;
