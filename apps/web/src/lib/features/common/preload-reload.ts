/**
 * 배포가 바뀌어 옛 청크를 못 읽을 때 한 번만 다시 읽기(Storybook 미리보기와 앱 본체).
 *
 * 빌드 파일은 이름에 내용 해시가 붙고, 브랜치 미리보기·배포는 같은 주소에 새 해시 파일로 덮어쓴다. 이미 열린 탭은 옛
 * 파일 목록을 들고 있어 화면·스토리를 바꿀 때 사라진 청크를 동적 import하다 404로 실패한다("Importing a module script
 * failed"). Vite는 이때 창에 `vite:preloadError`(취소 가능)를 보내고, 취소하지 않으면 그 오류를 import한 곳에 던진다.
 *
 * 여기서는 처음 한 번만 취소하고 다시 읽는다. 다시 읽은 표지를 이 탭의 sessionStorage에 남겨, 다시 읽은 뒤에도 같은
 * 일이 나면(또는 표지를 쓸 수 없으면) 다시 읽지 않고 `onStale`로 알린다(무한 다시 읽기 방지). 그때는 취소하지 않아 오류가
 * 원래대로 import한 곳에 간다(취소하면 import가 `undefined`로 끝나 엉뚱한 곳에서 깨진다).
 */
export const PRELOAD_RELOAD_KEY = 'fvoci:preload-reload';

export interface PreloadReloadOptions {
  /**
   * 다시 읽기 직전에 부른다. 거짓이면 다시 읽지 않는다(앱: 미저장 입력을 모두 초안으로 남기지 못했다). 다시 읽기 표지는
   * 쓰지 않은 것으로 되돌린다.
   */
  ready?: () => boolean;
  /** 다시 읽지 않기로 했을 때(이미 한 번 다시 읽었다, 표지를 쓸 수 없다, `ready`가 거짓). */
  onStale?: () => void;
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

  const onError = (event: Event): void => {
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
    reload();
  };

  window.addEventListener('vite:preloadError', onError);
  return () => {
    window.removeEventListener('vite:preloadError', onError);
  };
}
