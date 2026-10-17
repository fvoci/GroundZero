<script lang="ts">
  // 토스트 자리와 연결 배너(D-82). 넓은 화면은 오른쪽 아래, 좁은 화면은 위쪽 가운데에 토스트를 둬 화면 아래 조작부와
  // 가상 키보드에 가리지 않게 한다. 토스트는 닫기 단추가 있고 Alt+T로 닿는다(svelte-sonner). 배너는 화면 낭독기에 알린다.
  import { IsMobile } from '$lib/hooks/is-mobile.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { Toaster } from '$lib/ui/sonner';

  import { connection, watchConnection } from './feedback.svelte';

  const mobile = new IsMobile();

  $effect(() => watchConnection());
</script>

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
