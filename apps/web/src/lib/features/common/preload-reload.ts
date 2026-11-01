/**
 * 배포가 바뀌어 옛 청크를 못 읽을 때 실패 묶음마다 한 번만 다시 읽기(Storybook 미리보기와 앱 본체).
 *
 * 빌드 파일은 이름에 내용 해시가 붙고, 브랜치 미리보기·배포는 같은 주소에 새 해시 파일로 덮어쓴다. 이미 열린 탭은 옛
 * 파일 목록을 들고 있어 화면·스토리를 바꿀 때 사라진 청크를 동적 import하다 404로 실패한다("Importing a module script
 * failed"). Vite는 이때 창에 `vite:preloadError`(취소 가능)를 보내고, 취소하지 않으면 그 오류를 import한 곳에 던진다.
 *
 * 처음 실패하면 취소하고 다시 읽으며, 다시 읽었다는 표지를 이 탭의 sessionStorage에 남긴다. 다시 읽어 뜬 페이지는 표지가
 * 있으면 뜬 때부터 `PRELOAD_RELOAD_SETTLE_MS` 동안 그 실패 묶음 안에 있다고 본다. 그사이 또 실패하면(고친 뒤에도 깨진
 * 배포) 다시 읽지 않고 `onStale`로 알린다(무한 다시 읽기 방지). 그 시간을 넘겨 살아 있으면 표지를 지워 다음 배포에서 또
 * 한 번 다시 읽게 한다(오래 연 탭이 배포를 여러 번 만난다). 시간은 표지를 쓴 때가 아니라 새 페이지가 뜬 때부터 재므로,
 * 느린 회선에서 늦게 뜬 페이지도 깨진 배포에서 다시 읽기를 되풀이하지 않는다. 표지를 읽거나 쓸 수 없으면 다시 읽지 않는다.
 *
 * 다시 읽지 않을 때는 취소하지 않아 오류가 원래대로 import한 곳에 간다(취소하면 import가 `undefined`로 끝나 엉뚱한 곳에서
 * 깨진다). 부른 다시 읽기가 `PRELOAD_RELOAD_STALL_MS` 안에 페이지를 내리지 않으면(바깥 beforeunload에 막힘 등) 다시 읽는
 * 중 상태를 되돌리고 `onStale`로 알린다.
 */
export const PRELOAD_RELOAD_KEY = 'fvoci:preload-reload';

const localErrorHandlers = new Set<(error: unknown) => boolean>();

/** Return true only for an error whose feedback/retry the caller owns. Dispose after loading. */
export function registerPreloadErrorHandler(handler: (error: unknown) => boolean): () => void {
  localErrorHandlers.add(handler);
  return () => {
    localErrorHandlers.delete(handler);
  };
}

/** 다시 읽어 뜬 페이지가 같은 실패 묶음으로 보는 시간. 이 안의 실패는 다시 읽지 않는다. */
export const PRELOAD_RELOAD_SETTLE_MS = 30_000;

/**
 * 다시 읽기를 부른 뒤 페이지가 내려가기(`pagehide`)를 기다리는 시간. 그 안에 내려가지 않으면 다시 읽기가 일어나지 않은
 * 것으로 보고(바깥 beforeunload가 막아 사용자가 머무름 등) 되돌린다. 응답이 늦어 이 시간 뒤에 내려가도 되돌린 쪽이 더
 * 보수적이다(그사이의 실패는 취소하지 않고 배너를 띄울 뿐이다).
 * 다만 첫 타이머가 늦게 깨면 beforeunload 대화상자 등으로 주 스레드가 멈췄다고 보고 한 번 더 기다린다.
 * 두 번째 타이머는 늦게 깨도 되돌려, 배경 탭 타이머 조절 등으로 다시 읽는 중 상태가 계속 남지 않게 한다.
 */
export const PRELOAD_RELOAD_STALL_MS = 5_000;

/** 이보다 더 늦게 깬 stall 타이머는 주 스레드가 막혔던 것으로 본다. */
const PRELOAD_RELOAD_STALL_DELAY_MS = 1_000;

export interface PreloadReloadOptions {
  /**
   * 다시 읽기 직전에 부른다. 거짓이면 다시 읽지 않는다(앱: 미저장 입력을 모두 초안으로 남기지 못했다). 다시 읽기 표지는
   * 쓰지 않은 것으로 되돌린다.
   */
  ready?: () => boolean;
  /**
   * 다시 읽지 않기로 했을 때(같은 실패 묶음이다, 표지를 읽거나 쓸 수 없다, `ready`가 거짓, 부른 다시 읽기가 일어나지
   * 않았다).
   */
  onStale?: () => void;
  /** 부른 다시 읽기가 일어나지 않아 되돌릴 때 `ready`가 켠 것을 끈다(앱: 초안 남기기 모드). `onStale`보다 먼저. */
  cancelReload?: () => void;
  reload?: () => void;
  storage?: () => Storage;
}

/** `vite:preloadError`를 듣는다. 정리 함수를 돌려준다. */
export function watchPreloadErrors(options: PreloadReloadOptions = {}): () => void {
  const storage = options.storage ?? ((): Storage => window.sessionStorage);
  const reload =
    options.reload ??
    ((): void => {
      window.location.reload();
    });
  // 한 import가 여러 번 보낼 수 있다(미리 읽을 파일마다, 그리고 모듈 자체). 다시 읽기로 한 뒤로는 모두 취소한다.
  let reloading = false;

  const mark = (): boolean => {
    try {
      const store = storage();
      if (store.getItem(PRELOAD_RELOAD_KEY) !== null) return false;
      store.setItem(PRELOAD_RELOAD_KEY, new Date().toISOString());
      return true;
    } catch {
      return false;
    }
  };
  const unmark = (): void => {
    try {
      storage().removeItem(PRELOAD_RELOAD_KEY);
    } catch {
      // 표지를 지우지 못하면 다음에는 다시 읽지 않고 알린다(더 보수적이다).
    }
  };

  // 다시 읽어 뜬 페이지(또는 일어나지 않은 다시 읽기를 되돌린 페이지): 그때부터 잰 시간이 지나면 이 실패 묶음이
  // 끝났다고 보고 표지를 지운다.
  let settle: ReturnType<typeof setTimeout> | undefined;
  const settleLater = (): void => {
    if (settle !== undefined) clearTimeout(settle);
    settle = setTimeout(() => {
      settle = undefined;
      if (!reloading) unmark();
    }, PRELOAD_RELOAD_SETTLE_MS);
  };
  try {
    if (storage().getItem(PRELOAD_RELOAD_KEY) !== null) settleLater();
  } catch {
    // 표지를 읽을 수 없으면 실패할 때 `mark()`도 거짓이라 다시 읽지 않는다.
  }

  // 부른 다시 읽기가 정말 페이지를 내리는지 본다. 내리지 않으면 다시 읽는 중 상태를 풀어, 이후 실패를 조용히 취소하지
  // 않게 한다(C-1). 표지는 남겨 같은 실패 묶음으로 보고, 위와 같이 시간이 지나면 지운다.
  let stall: ReturnType<typeof setTimeout> | undefined;
  const onPageHide = (): void => {
    if (stall !== undefined) clearTimeout(stall);
    stall = undefined;
  };
  const watchReload = (): void => {
    window.addEventListener('pagehide', onPageHide, { once: true });
    let waitedAgain = false;
    const waitForPageHide = (): void => {
      const expectedAt = performance.now() + PRELOAD_RELOAD_STALL_MS;
      stall = setTimeout(() => {
        stall = undefined;
        if (!waitedAgain && performance.now() - expectedAt > PRELOAD_RELOAD_STALL_DELAY_MS) {
          // 대화상자에서 떠나기를 골랐어도 pagehide보다 밀린 타이머가 먼저 돌 수 있다(C-1·#164).
          waitedAgain = true;
          waitForPageHide();
          return;
        }
        window.removeEventListener('pagehide', onPageHide);
        reloading = false;
        options.cancelReload?.();
        options.onStale?.();
        settleLater();
      }, PRELOAD_RELOAD_STALL_MS);
    };
    waitForPageHide();
  };

  const onError = (event: Event): void => {
    // Window-targeted dispatch invokes listeners in registration order, even for
    // capture listeners. Consult local renderers before any reload/draft side effects.
    const error = 'payload' in event ? event.payload : undefined;
    for (const handler of localErrorHandlers) {
      if (!handler(error)) continue;
      event.preventDefault();
      return;
    }
    if (reloading) {
      event.preventDefault();
      return;
    }
    if (!mark()) {
      options.onStale?.();
      return;
    }
    if (options.ready?.() === false) {
      unmark();
      options.onStale?.();
      return;
    }
    reloading = true;
    event.preventDefault();
    watchReload();
    reload();
  };

  window.addEventListener('vite:preloadError', onError);
  return () => {
    if (settle !== undefined) clearTimeout(settle);
    if (stall !== undefined) clearTimeout(stall);
    window.removeEventListener('pagehide', onPageHide);
    window.removeEventListener('vite:preloadError', onError);
  };
}
