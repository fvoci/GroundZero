<script lang="ts">
  // 토스트 자리와 연결·로그인 배너(D-82). 넓은 화면은 오른쪽 아래, 좁은 화면은 위쪽 가운데에 토스트를 둬 화면 아래
  // 조작부와 가상 키보드에 가리지 않게 한다. 토스트는 닫기 단추가 있고 Alt+T로 닿는다(svelte-sonner). 배너는 화면
  // 낭독기에 알린다.
  import { api } from '$lib/api/client';
  import { IsMobile } from '$lib/hooks/is-mobile.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { router } from '$lib/router.svelte';
  import { Toaster } from '$lib/ui/sonner';

  import { auth, connection, watchConnection } from './feedback.svelte';

  const mobile = new IsMobile();

  $effect(() => watchConnection());

  // 세션이 끝난 동안 이 창으로 돌아오면(다른 탭에서 로그인했을 수 있다) 다시 확인한다. 로그인돼 있으면
  // client.ts가 성공한 응답을 보고 배너를 내린다.
  $effect(() => {
    const recheck = (): void => {
      if (auth.expired) void api.me().catch(() => undefined);
    };
    window.addEventListener('focus', recheck);
    return () => {
      window.removeEventListener('focus', recheck);
    };
  });

  // 새 탭에서 로그인한 뒤 같은 화면으로 돌아오게 한다. 이 창은 다시 읽지 않아 입력이 남는다.
  const signInHref = $derived(
    `/auth/login?return_to=${encodeURIComponent(router.path + router.hash)}`,
  );
</script>

{#if auth.expired}
  <div
    role="status"
    aria-live="polite"
    class="sticky top-0 z-40 flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-amber-300 bg-amber-50 px-4 py-2 text-sm text-amber-900"
    data-testid="session-banner"
  >
    <span>{m.common_session_expired()}</span>
    <a class="font-medium underline" href={signInHref} target="_blank" rel="noopener noreferrer"
      >{m.common_session_sign_in()}</a
    >
  </div>
{/if}
{#if connection.lost}
  <div
    role="status"
    aria-live="polite"
    class="sticky top-0 z-40 border-b border-amber-300 bg-amber-50 px-4 py-2 text-sm text-amber-900"
    data-testid="connection-banner"
  >
    {m.common_connection_lost()}
  </div>
{/if}
<Toaster
  position={mobile.current ? 'top-center' : 'bottom-right'}
  closeButton
  visibleToasts={3}
  containerAriaLabel={m.common_notifications_region()}
/>
