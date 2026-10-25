// 사용자 피드백 규칙(D-82). 일반 완료는 토스트, 입력 오류는 필드 옆, 저장 실패·충돌은 그 화면에 남는 메시지, 연결
// 상태는 배너, 멘션·초대는 알림함이다. 이 모듈은 토스트와 연결·로그인 배너만 맡는다(나머지는 각 화면이 그 자리에서
// 보인다).
// 문구는 부르는 쪽이 i18n 메시지로 넘긴다(D-81).
import { toast } from 'svelte-sonner';
import { flushSync } from 'svelte';

/**
 * 완료 알림. 같은 `key`(없으면 문구)는 쌓지 않고 하나를 새로 고친다. 화면 낭독기는 토스트 영역(aria-live)으로 읽는다.
 */
export function notifyDone(message: string, key: string = message): void {
  toast.success(message, { id: key });
}

/** 같은 실패는 한 토스트에서 갱신한다. 저장 오류는 호출 화면에 남긴다(D-82). */
export function notifyError(message: string, key: string = message): void {
  toast.error(message, { id: key });
}

/** 연결 상태(배너). 브라우저가 오프라인이거나 API가 응답하지 않으면 `lost`. */
export const connection = $state({ lost: false });

let probing: ReturnType<typeof setInterval> | undefined;

/** 서버에 닿을 때까지 몇 초마다 확인하고, 닿으면 배너를 내린다. */
function probe(): void {
  if (probing !== undefined) return;
  probing = setInterval(() => {
    fetch('/healthz', { cache: 'no-store' })
      .then((r) => {
        if (r.ok && navigator.onLine) {
          connection.lost = false;
          if (probing !== undefined) clearInterval(probing);
          probing = undefined;
        }
      })
      .catch(() => undefined);
  }, 5_000);
}

/**
 * 로그인 상태(배너). 쓰는 중에 세션이 끝나(만료·다른 탭에서 로그아웃) API가 401을 돌려주면 `expired`. 화면을 다시
 * 읽지 않고 새 탭에서 로그인하게 해 이 창의 입력을 지킨다. 세션이 필요한 요청이 다시 성공하면 내린다.
 */
export const auth = $state({ expired: false });

// 401을 알린 횟수. 요청은 시작할 때 이 값을 적어 두고, 그 뒤 401이 없었을 때만 성공으로 배너를 내린다(401보다 먼저
// 시작한 요청의 늦은 성공은 세션이 돌아왔다는 뜻이 아니다).
let expiries = 0;

/** 세션이 필요한 API가 401을 돌려줬을 때 부른다(처음 로그인 확인 `/api/me`는 제외). */
export function reportSessionExpired(): void {
  expiries += 1;
  auth.expired = true;
}

/** 지금까지 알린 세션 만료(401) 횟수. */
export function sessionExpiries(): number {
  return expiries;
}

/** API가 응답 없이 실패했을 때(NetworkError) 부른다. */
export function reportConnectionLost(): void {
  connection.lost = true;
  probe();
}

/** 브라우저 온라인·오프라인 이벤트를 듣는다. 앱 시작 때 한 번. 정리 함수를 돌려준다. */
export function watchConnection(): () => void {
  const offline = (): void => {
    reportConnectionLost();
  };
  const online = (): void => {
    probe();
  };
  window.addEventListener('offline', offline);
  window.addEventListener('online', online);
  if (!navigator.onLine) reportConnectionLost();
  return () => {
    window.removeEventListener('offline', offline);
    window.removeEventListener('online', online);
    if (probing !== undefined) clearInterval(probing);
    probing = undefined;
  };
}

// 다른 계정으로 바뀌어 이 창을 처음부터 다시 읽는 중인지. 이때는 저장 안 한 편집 경고로 다시 읽기를 막지 않는다
// (막고 남으면 앞 계정의 화면과 입력이 새 계정의 세션으로 요청을 보낸다). 입력은 앞 계정의 초안(사용자별 키)으로
// 이미 남아 있다.
export const accountSafety = $state({ switching: false });

/** 다른 계정으로 바뀌어 다시 읽기 직전에 부른다. */
export function markAccountSwitch(): void {
  // reload가 beforeunload에서 취소되어도 이전 계정의 화면을 숨긴 상태로 남긴다.
  flushSync(() => {
    accountSafety.switching = true;
  });
}

/** 다른 계정으로 바뀌어 다시 읽는 중인지. */
export function accountSwitching(): boolean {
  return accountSafety.switching;
}

// 계정이 바뀐 것을 안 곳(창 복귀 확인 밖, 예: 문서의 배경 저장)에서 이 창을 다시 읽는 방법. FeedbackHost가 정한다.
let reloadForSwitch: (() => void) | undefined;

/** 계정이 바뀌었을 때 이 창을 다시 읽는 방법을 정한다(FeedbackHost). 정리 함수를 돌려준다. */
export function onAccountSwitch(reload: () => void): () => void {
  reloadForSwitch = reload;
  return () => {
    if (reloadForSwitch === reload) reloadForSwitch = undefined;
  };
}

/** 다른 계정으로 바뀐 것을 알았을 때 부른다: 표시하고 이 창을 처음부터 다시 읽는다. */
export function switchAccount(): void {
  if (accountSwitching()) return;
  markAccountSwitch();
  if (reloadForSwitch !== undefined) reloadForSwitch();
  else window.location.reload();
}
