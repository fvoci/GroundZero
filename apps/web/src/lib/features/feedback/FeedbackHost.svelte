<script lang="ts">
  // 토스트 자리와 연결·로그인 배너(D-82). 넓은 화면은 오른쪽 아래, 좁은 화면은 위쪽 가운데에 토스트를 둬 화면 아래
  // 조작부와 가상 키보드에 가리지 않게 한다. 토스트는 닫기 단추가 있고 Alt+T로 닿는다(svelte-sonner). 배너는 화면
  // 낭독기에 알린다.
  import { api } from '$lib/api/client';
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
  } from './feedback.svelte';

  interface Props {
    /** 이 창을 처음부터 다시 읽는다(시험에서 바꿔 끼운다). */
    reload?: () => void;
    replace?: (url: string) => void;
    retryLogout?: () => void;
    retryRestore?: () => void;
  }

  const {
    replace = (url) => {
      window.location.replace(url);
    },
    retryLogout,
    retryRestore,
    reload = () => {
      window.location.reload();
    },
  }: Props = $props();

  const mobile = new IsMobile();
  // Dialog의 portal도 가린다. 편집 DOM은 남겨 메모리의 입력을 보존한다.
  $effect(() => {
    document.documentElement.toggleAttribute('data-account-switching', accountSafety.switching);
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
  // 두 이벤트가 잇달아 와도 확인 요청은 한 번에 하나만 보내고(`checking`), 전환은 `switchAccount()`가 한 번만 시작한다.
  $effect(() => {
    let checking = false;
    let active = true;
    const recheck = (): void => {
      const signedIn = currentUser.userId;
      if (signedIn === '' || accountSafety.switching || checking) return;
      checking = true;
      void api
        .me()
        .then((now) => {
          if (!active || accountSafety.switching || currentUser.userId !== signedIn) return;
          if (now === undefined) {
            hideSession('signed-out');
            replace(router.path + router.hash);
          } else if (now.id !== signedIn) switchAccount();
        })
        .catch(() => undefined)
        .finally(() => {
          checking = false;
        });
    };
    const onVisible = (): void => {
      if (document.visibilityState === 'visible') recheck();
    };
    window.addEventListener('focus', recheck);
    document.addEventListener('visibilitychange', onVisible);
    return () => {
      active = false;
      window.removeEventListener('focus', recheck);
      document.removeEventListener('visibilitychange', onVisible);
    };
  });

  // 새 탭에서 로그인한 뒤 같은 화면으로 돌아오게 한다. 이 창은 다시 읽지 않아 입력이 남는다.
  const signInHref = $derived(
    `/auth/login?return_to=${encodeURIComponent(router.path + router.hash)}`,
  );
</script>

<TimerRestartDialog />

<!-- 이미 열린 대화상자의 초점 덫 위에 안전 상태를 연다. 기존 편집 상태는 그대로 보존한다. -->
<Dialog.Root open={accountSafety.switching}>
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
        {:else if accountSafety.reason === 'restore'}{m.feedback_restore_title()}
        {:else if accountSafety.reason === 'signed-out'}{m.feedback_signed_out_title()}
        {:else}{m.feedback_account_changed_title()}{/if}
      </Dialog.Title>
      <Dialog.Description>
        {#if accountSafety.reason === 'logout'}{m.feedback_logout_pending()}
        {:else if accountSafety.reason === 'restore'}{m.feedback_restore_pending()}
        {:else if accountSafety.reason === 'signed-out'}{m.feedback_signed_out_pending()}
        {:else}{m.feedback_account_switching()}{/if}
      </Dialog.Description>
    </Dialog.Header>
    {#if accountSafety.error !== ''}<p role="alert">{accountSafety.error}</p>{/if}
    {#if accountSafety.reason === 'logout'}
      {#if accountSafety.error !== '' && retryLogout !== undefined}
        <Button onclick={retryLogout}>{m.feedback_logout_retry()}</Button>
      {/if}
    {:else if accountSafety.reason === 'restore'}
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
  {#if auth.expired && !sessionDismissed && !accountSafety.switching}
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
  {#if connection.lost && !connectionDismissed && !accountSafety.switching}
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
  {#if newVersion.times > versionDismissed && !accountSafety.switching}
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
