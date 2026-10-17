// 사용자 피드백 규칙(D-82). 일반 완료는 토스트, 입력 오류는 필드 옆, 저장 실패·충돌은 그 화면에 남는 메시지, 연결
// 상태는 배너, 멘션·초대는 알림함이다. 이 모듈은 토스트와 연결 배너만 맡는다(나머지는 각 화면이 그 자리에서 보인다).
// 문구는 부르는 쪽이 i18n 메시지로 넘긴다(D-81).
import { toast } from 'svelte-sonner';

/**
 * 완료 알림. 같은 `key`(없으면 문구)는 쌓지 않고 하나를 새로 고친다. 화면 낭독기는 토스트 영역(aria-live)으로 읽는다.
 */
export function notifyDone(message: string, key: string = message): void {
  toast.success(message, { id: key });
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
