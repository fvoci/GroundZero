import { accountSwitching } from '$lib/features/feedback/feedback.svelte';
import { router } from '$lib/router.svelte';
import { SvelteSet } from 'svelte/reactivity';

type NavigationGuard = (action: () => void, next?: string) => boolean;
const attached = new SvelteSet<{ guard: NavigationGuard; previous: NavigationGuard | undefined }>();

// 붙어 있는 모든 가드의 창 떠나기 확인. 각 확인은 미저장 입력을 초안으로 남기고(D-27), 떠나기를 막아야 하면 참이다.
const unloadChecks = new SvelteSet<() => boolean>();
let keepingForReload = false;

/**
 * 이 창을 곧 다시 읽어 미저장 입력이 초안으로만 이어지는 중인지: 다른 계정으로 바뀌었거나(D-96) 새 버전 배포로 다시
 * 읽는다. 이때 창 떠나기 확인은 "남기지 못한 입력이 있는지"만 답한다(남겼으면 막지 않는다).
 */
export function leavingWithDrafts(): boolean {
  return keepingForReload || accountSwitching();
}

/**
 * 붙어 있는 모든 이탈 가드·초안의 남기기를 한 번에 부르고, 모두 남겼으면(남길 것이 없으면) 참. 참이면 곧 다시 읽으므로
 * `leavingWithDrafts()`를 켠 채 두어 다시 읽을 때의 beforeunload가 남긴 입력으로 막지 않게 한다. 하나라도 남기지
 * 못했으면 끄고 거짓(다시 읽지 말 것).
 */
export function keepAllDrafts(): boolean {
  keepingForReload = true;
  // 앞의 가드가 막아도 뒤의 가드까지 모두 남기게 한다.
  const blocked = [...unloadChecks].map((check) => check());
  if (blocked.includes(true)) keepingForReload = false;
  return keepingForReload;
}

/** `keepAllDrafts()` 뒤 다시 읽기가 일어나지 않았을 때(바깥 beforeunload가 막음, `new-version.ts`) 표지를 내린다. */
export function stopKeepingDrafts(): void {
  keepingForReload = false;
}

interface LeaveOptions {
  dirty: () => boolean;
  busy?: () => boolean;
  /** 다른 편집 세션을 뒤늦은 확인으로 닫지 않는다. */
  identity?: () => unknown;
  /** 확인을 열기 직전 초점 등 화면 문맥을 기억한다. */
  onAsk?: () => void;
  /**
   * 앱 안 이동의 다음 경로에서도 이 폼이 마운트된 채인지(항목 머리 폼은 묶음만 바뀌면 그대로다, D-121). true면 묻지
   * 않고 다음 가드로 넘긴다. 경로를 모르는 이동(문맥 전환·기록 뒤로 가기)과 beforeunload는 그대로 묻는다.
   */
  stays?: (next: string) => boolean;
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
    /**
     * 떠나기를 막아야 하면 true. 초안 보존은 호출 화면이 수행한다. `leavingWithDrafts()`가 참이면 남긴 입력으로는
     * 막지 않고 남기지 못한 입력이 있을 때만 true다(`keepAllDrafts()`도 이 확인을 부른다).
     */
    beforeUnload?: () => boolean;
  } = {}): () => void {
    const link: { guard: NavigationGuard; previous: NavigationGuard | undefined } = {
      guard: () => true,
      previous: router.guard,
    };
    const guard = (action: () => void, next?: string): boolean => {
      if (bypass) return link.previous?.(action, next) ?? true;
      // The form stays mounted at the next path, so its input is not left behind.
      if (next !== undefined && options.stays?.(next) === true)
        return link.previous?.(action, next) ?? true;
      if (options.busy?.() === true) return false;
      if (!options.dirty()) return link.previous?.(action, next) ?? true;
      // Carry each accepted guard's bypass into the remaining confirmations.
      // Otherwise a sibling's later confirmation would ask this dirty form again.
      const resume = (): void => {
        proceed(action);
      };
      request(() => {
        if (link.previous?.(resume, next) ?? true) resume();
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
    // 같은 함수를 둘이 붙여도(기본값 `options.dirty`) 따로 셈하도록 붙일 때마다 새로 감싼다.
    const check = (): boolean => beforeUnload();
    unloadChecks.add(check);
    return () => {
      unloadChecks.delete(check);
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
