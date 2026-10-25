import { router } from '$lib/router.svelte';

interface LeaveOptions {
  dirty: () => boolean;
  busy?: () => boolean;
  /** 다른 편집 세션을 뒤늦은 확인으로 닫지 않는다. */
  identity?: () => unknown;
  /** 확인을 열기 직전 초점 등 화면 문맥을 기억한다. */
  onAsk?: () => void;
}

/** DiscardChangesConfirm의 open/onkeep/ondiscard와 연결한다(D-97·D-125). */
export function createLeaveGuard(options: LeaveOptions) {
  let pending = $state<(() => void) | undefined>();
  let asked: unknown;
  let bypass = false;
  function keep(): void {
    pending = undefined;
  }
  function proceed(action: () => void): void {
    bypass = true;
    try {
      action();
    } finally {
      bypass = false;
    }
  }
  function request(action: () => void): void {
    if (options.busy?.() === true || pending !== undefined) return;
    if (!options.dirty()) {
      proceed(action);
      return;
    }
    asked = options.identity?.();
    options.onAsk?.();
    pending = action;
  }
  function discard(): void {
    if (options.busy?.() === true) return;
    const action = pending;
    keep();
    if (action !== undefined && asked === options.identity?.()) proceed(action);
  }
  /** onMount에서 호출하고 반환한 정리 함수를 돌려준다. */
  function attach({
    navigation = true,
    beforeUnload = options.dirty,
  }: {
    navigation?: boolean;
    /** 떠나기를 막아야 하면 true. 초안 보존은 호출 화면이 수행한다. */
    beforeUnload?: () => boolean;
  } = {}): () => void {
    const previous = router.guard;
    const guard = (action: () => void): boolean => {
      if (bypass) return true;
      if (options.busy?.() === true) return false;
      if (!options.dirty()) return previous?.(action) ?? true;
      request(action);
      return false;
    };
    const unload = (event: BeforeUnloadEvent): void => {
      if (beforeUnload()) event.preventDefault();
    };
    if (navigation) router.guard = guard;
    window.addEventListener('beforeunload', unload);
    return () => {
      if (navigation && router.guard === guard) router.guard = previous;
      window.removeEventListener('beforeunload', unload);
      keep();
    };
  }
  return {
    get open() {
      return pending !== undefined;
    },
    request,
    keep,
    discard,
    proceed,
    attach,
  };
}
