<script lang="ts">
  // 토스트 자리와 연결·로그인 배너(D-82). 넓은 화면은 오른쪽 아래, 좁은 화면은 위쪽 가운데에 토스트를 둬 화면 아래
  // 조작부와 가상 키보드에 가리지 않게 한다. 토스트는 닫기 단추가 있고 Alt+T로 닿는다(svelte-sonner). 배너는 화면
  // 낭독기에 알린다.
  import { api } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { SESSION_CHECK_MAX_ATTEMPTS } from '$lib/features/auth/session-check';
  import { publishSessionUser, readSessionGeneration } from '$lib/features/auth/session-signal';
  import { IsMobile } from '$lib/hooks/is-mobile.svelte';
  import { me as currentUser } from '$lib/me.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { router } from '$lib/router.svelte';
  import { Toaster } from '$lib/ui/sonner';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';
  import TimerRestartDialog from '$lib/features/time/TimerRestartDialog.svelte';

  import {
    auth,
    accountSafety,
    connection,
    newVersion,
    switchAccount,
    onAccountSwitch,
    watchConnection,
    hideSession,
    reportSessionExpired,
  } from './feedback.svelte';

  interface Props {
    /** 이 창을 처음부터 다시 읽는다(시험에서 바꿔 끼운다). */
    reload?: () => void;
    retryLogout?: () => void;
    retryRestore?: () => void;
    /** 셸이 시작·복귀·복원의 확인을 한 대기열로 관리한다. */
    sessionManaged?: boolean;
  }

  const {
    retryLogout,
    retryRestore,
    sessionManaged = false,
    reload = () => {
      window.location.reload();
    },
  }: Props = $props();

  const mobile = new IsMobile();
  const sessionHidden = $derived(accountSafety.switching || accountSafety.restoring);
  // Dialog의 portal도 가린다. 편집 DOM은 남겨 메모리의 입력을 보존한다.
  $effect(() => {
    document.documentElement.toggleAttribute('data-account-switching', sessionHidden);
    return () => {
      document.documentElement.removeAttribute('data-account-switching');
    };
  });
  let sessionDismissed = $state(false);
  let connectionDismissed = $state(false);
  $effect(() => {
    if (!auth.expired) sessionDismissed = false;
  });
  $effect(() => {
    if (!connection.lost) connectionDismissed = false;
  });
  // 닫은 뒤 다시 옛 청크를 못 읽으면 다시 보인다.
  let versionDismissed = $state(0);

  $effect(() => watchConnection());
  // 화면(문서의 배경 저장 등)이 계정이 바뀐 것을 알면 아래 확인과 같게 다시 읽는다.
  $effect(() => onAccountSwitch(reload));
  // 뒤로 가기나 늦은 이동이 숨겨 둔 편집 확인창을 새로 열지 못하게 한다.
  $effect(() => {
    if (!accountSafety.switching || accountSafety.reason === 'restore') return;
    const previous = router.guard;
    const stay = (): boolean => false;
    router.guard = stay;
    return () => {
      if (router.guard === stay) router.guard = previous;
    };
  });

  // 이 창으로 돌아올 때마다 누구로 로그인돼 있는지 다시 확인한다(다른 탭에서 로그아웃하고 다른 계정으로 로그인했을 수
  // 있다. 세션이 끝난 동안이 아니어도 쿠키는 이미 바뀌었다). 같은 계정이면 화면과 입력은 그대로 두고, 세션이 끝난
  // 동안이었으면 client.ts가 성공한 응답을 보고 배너를 내린다. 다른 계정이면 이 창의 화면·입력·상태는 앞 계정의
  // 것이므로 남기지 않고 처음부터 다시 읽는다. 아직 로그인 전인 창은 확인하지 않는다.
  //
  // 감지 시점(#88): 창 `focus`와 문서가 다시 보이는 `visibilitychange` 둘 다 쓴다. 키보드·OS로 탭을 바꾸면 창 focus 없이
  // 탭만 보이게 될 수 있고, 헤드리스 Chromium의 탭 전환은 focus를 내지 않았다(e2e account-switch의 focus-path 기록).
  // 확인 중 복귀 이벤트는 후속 확인 하나로 합치고(SR-221-10), 전환은 `switchAccount()`가 한 번만 시작한다.
  $effect(() => {
    if (sessionManaged) return;
    let checking = false;
    let pending = false;
    let active = true;
    const recheck = (): void => {
      const signedIn = currentUser.userId;
      if (!active || signedIn === '' || (sessionHidden && accountSafety.reason !== 'reauth'))
        return;
      if (checking) {
        pending = true;
        return;
      }
      const canCheck = (): boolean =>
        active &&
        currentUser.userId === signedIn &&
        (!(accountSafety.switching || accountSafety.restoring) ||
          accountSafety.reason === 'reauth' ||
          accountSafety.reason === 'restore');
      const refresh = (): boolean => {
        if (!canCheck()) return false;
        hideSession('reauth');
        return true;
      };
      checking = true;
      const verify = async (): Promise<void> => {
        for (let attempt = 0; attempt < SESSION_CHECK_MAX_ATTEMPTS; attempt += 1) {
          if (!canCheck()) return;
          const generation = accountSafety.generation;
          const sessionGeneration = readSessionGeneration();
          try {
            const now = await api.me();
            if (!canCheck()) return;
            if (generation !== accountSafety.generation) {
              if (refresh()) continue;
              return;
            }
            void publishSessionUser(now?.id ?? null, sessionGeneration);
            if (now === undefined) {
              reportSessionExpired();
              hideSession('reauth');
            }
            if (now === undefined) return;
            if (now.id !== signedIn) switchAccount();
            else {
              if (accountSafety.reason === 'reauth') {
                accountSafety.switching = false;
                accountSafety.restoring = false;
                auth.expired = false;
              }
            }
            return;
          } catch (error: unknown) {
            if (!canCheck()) return;
            if (generation !== accountSafety.generation) {
              if (refresh()) continue;
              return;
            }
            if (accountSafety.switching || accountSafety.restoring)
              accountSafety.error = errorText(error, m.feedback_restore_failed());
            return;
          }
        }
        if (canCheck()) {
          hideSession('reauth');
          accountSafety.error = m.feedback_restore_failed();
        }
      };
      void verify().finally(() => {
        checking = false;
        if (pending) {
          pending = false;
          recheck();
        }
      });
    };
    const onVisible = (): void => {
      if (document.visibilityState === 'visible') recheck();
    };
    window.addEventListener('focus', recheck);
    document.addEventListener('visibilitychange', onVisible);
    window.addEventListener('pageshow', recheck);
    return () => {
      active = false;
      pending = false;
      window.removeEventListener('focus', recheck);
      document.removeEventListener('visibilitychange', onVisible);
      window.removeEventListener('pageshow', recheck);
    };
  });

  // 새 탭에서 로그인한 뒤 같은 화면으로 돌아오게 한다. 이 창은 다시 읽지 않아 입력이 남는다.
  const signInHref = $derived(
    `/auth/login?return_to=${encodeURIComponent(router.path + router.hash)}`,
  );
</script>

<TimerRestartDialog />

<!-- 이미 열린 대화상자의 초점 덫 위에 안전 상태를 연다. 기존 편집 상태는 그대로 보존한다. -->
<Dialog.Root open={sessionHidden}>
  <Dialog.Content
    data-account-safe
    data-testid="account-safe-state"
    class="max-h-[calc(100dvh-2rem)] grid-cols-[minmax(0,1fr)] overflow-y-auto"
    showCloseButton={false}
    escapeKeydownBehavior="ignore"
    interactOutsideBehavior="ignore"
  >
    <Dialog.Header>
      <Dialog.Title>
        {#if accountSafety.reason === 'logout'}{m.app_menu_logout()}
        {:else if accountSafety.reason === 'reauth' && auth.expired}{m.common_session_sign_in()}
        {:else if accountSafety.reason === 'restore' || accountSafety.reason === 'reauth'}{m.feedback_restore_title()}
        {:else if accountSafety.reason === 'signed-out'}{m.feedback_signed_out_title()}
        {:else}{m.feedback_account_changed_title()}{/if}
      </Dialog.Title>
      <Dialog.Description>
        {#if accountSafety.reason === 'logout'}{m.feedback_logout_pending()}
        {:else if accountSafety.reason === 'reauth' && auth.expired}{m.common_session_expired()}
        {:else if accountSafety.reason === 'restore' || accountSafety.reason === 'reauth'}{m.feedback_restore_pending()}
        {:else if accountSafety.reason === 'signed-out'}{m.feedback_signed_out_pending()}
        {:else}{m.feedback_account_switching()}{/if}
      </Dialog.Description>
    </Dialog.Header>
    {#if accountSafety.error !== ''}<p role="alert">{accountSafety.error}</p>{/if}
    {#if accountSafety.reason === 'logout'}
      {#if accountSafety.error !== '' && retryLogout !== undefined}
        <Button onclick={retryLogout}>{m.feedback_logout_retry()}</Button>
      {/if}
    {:else if accountSafety.reason === 'restore' || accountSafety.reason === 'reauth'}
      {#if accountSafety.reason === 'reauth' && auth.expired}
        <a class="font-medium underline" href={signInHref} target="_blank" rel="noopener noreferrer"
          >{m.common_session_sign_in()}</a
        >
      {/if}
      {#if accountSafety.error !== '' && retryRestore !== undefined}
        <Button onclick={retryRestore}>{m.common_retry()}</Button>
      {/if}
    {:else}
      <Button class="h-auto max-w-full whitespace-normal" onclick={reload}
        >{m.feedback_reload()}</Button
      >
    {/if}
  </Dialog.Content>
</Dialog.Root>
<div role="status" aria-live="polite" class="empty:hidden">
  {#if auth.expired && !sessionDismissed && !sessionHidden}
    <div
      class="sticky top-0 z-40 flex flex-wrap items-center gap-x-3 gap-y-1 border-b bg-secondary px-4 py-2 text-sm text-secondary-foreground"
      data-testid="session-banner"
    >
      <span>{m.common_session_expired()}</span>
      <a class="font-medium underline" href={signInHref} target="_blank" rel="noopener noreferrer"
        >{m.common_session_sign_in()}</a
      >
      <Button variant="ghost" size="sm" onclick={() => (sessionDismissed = true)}
        >{m.common_close()}</Button
      >
    </div>
  {/if}
</div>
<div role="status" aria-live="polite" class="empty:hidden">
  {#if connection.lost && !connectionDismissed && !sessionHidden}
    <div
      class="sticky top-0 z-40 flex flex-wrap items-center gap-3 border-b bg-secondary px-4 py-2 text-sm text-secondary-foreground"
      data-testid="connection-banner"
    >
      {m.common_connection_lost()}
      <Button variant="ghost" size="sm" onclick={() => (connectionDismissed = true)}
        >{m.common_close()}</Button
      >
    </div>
  {/if}
</div>
<div role="status" aria-live="polite" class="empty:hidden">
  {#if newVersion.times > versionDismissed && !sessionHidden}
    <div
      class="sticky top-0 z-40 flex flex-wrap items-center gap-3 border-b bg-secondary px-4 py-2 text-sm text-secondary-foreground"
      data-testid="new-version-banner"
    >
      {m.feedback_new_version()}
      <Button variant="ghost" size="sm" onclick={() => (versionDismissed = newVersion.times)}
        >{m.common_close()}</Button
      >
    </div>
  {/if}
</div>
<Toaster
  position={mobile.current ? 'top-center' : 'bottom-right'}
  closeButton
  visibleToasts={3}
  containerAriaLabel={m.common_notifications_region()}
/>
