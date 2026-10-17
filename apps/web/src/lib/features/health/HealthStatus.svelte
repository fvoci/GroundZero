<script lang="ts">
  import { onMount } from 'svelte';

  import type { HealthResult } from '$lib/api/client';

  interface Props {
    load: () => Promise<HealthResult>;
  }

  const { load }: Props = $props();

  let result = $state<HealthResult | undefined>(undefined);

  onMount(() => {
    // 화면이 사라진 뒤 도착한 응답은 버린다 (오래된 응답 처리).
    let cancelled = false;
    load()
      .then((value) => {
        if (!cancelled) result = value;
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          const message = error instanceof Error ? error.message : 'unknown error';
          result = { kind: 'unreachable', message };
        }
      });
    return () => {
      cancelled = true;
    };
  });
</script>

<section aria-live="polite">
  {#if result === undefined}
    <p>서버 상태 확인 중…</p>
  {:else if result.kind === 'ok'}
    <p data-testid="health">서버 정상 (database: {result.health.database})</p>
  {:else if result.kind === 'degraded'}
    <p data-testid="health">서버 응답은 있으나 데이터베이스 연결 실패</p>
  {:else}
    <p data-testid="health">서버에 연결할 수 없음: {result.message}</p>
  {/if}
</section>
