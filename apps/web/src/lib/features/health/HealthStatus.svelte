<script lang="ts">
  import { onMount } from 'svelte';

  import type { HealthResult } from '$lib/api/client';
  import { m } from '$lib/paraglide/messages.js';

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
    <p>{m.app_health_checking()}</p>
  {:else if result.kind === 'ok'}
    <p data-testid="health">{m.app_health_ok({ database: result.health.database })}</p>
  {:else if result.kind === 'degraded'}
    <p data-testid="health">{m.app_health_degraded()}</p>
  {:else}
    <p data-testid="health">{m.app_health_unreachable({ message: result.message })}</p>
  {/if}
</section>
