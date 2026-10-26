import { router } from '$lib/router.svelte';
import { SvelteSet } from 'svelte/reactivity';

type NavigationGuard = (action: () => void) => boolean;
const attached = new SvelteSet<{ guard: NavigationGuard; previous: NavigationGuard | undefined }>();

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
    const link: { guard: NavigationGuard; previous: NavigationGuard | undefined } = {
      guard: () => true,
      previous: router.guard,
    };
    const guard = (action: () => void): boolean => {
      if (bypass) return link.previous?.(action) ?? true;
      if (options.busy?.() === true) return false;
      if (!options.dirty()) return link.previous?.(action) ?? true;
      // Carry each accepted guard's bypass into the remaining confirmations.
      // Otherwise a sibling's later confirmation would ask this dirty form again.
      const resume = (): void => {
        proceed(action);
      };
      request(() => {
        if (link.previous?.(resume) ?? true) resume();
      });
      return false;
    };
    const unload = (event: BeforeUnloadEvent): void => {
      if (beforeUnload()) event.preventDefault();
    };
    link.guard = guard;
    if (navigation) {
      attached.add(link);
      router.guard = guard;
    }
    window.addEventListener('beforeunload', unload);
    return () => {
      if (navigation) {
        if (router.guard === guard) router.guard = link.previous;
        // Siblings need not unmount in the reverse order of attachment.
        for (const other of attached) {
          if (other.previous === guard) other.previous = link.previous;
        }
        attached.delete(link);
      }
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
