<script lang="ts">
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  interface Props {
    onrestore: () => void;
    ondiscard: () => void;
    /** 지금 편집할 수 없으면(쓰기 불가·계정 전환 중) false. 되살리기 대신 안내만 보이고 입력은 버리기 전까지 남는다. */
    restorable?: boolean;
    restoreDisabled?: boolean;
  }
  const { onrestore, ondiscard, restorable = true, restoreDisabled = false }: Props = $props();
</script>

<div class="flex flex-wrap items-center gap-2 rounded border p-3" data-testid="form-draft-offer">
  <p class="text-sm">{m.common_draft_available()}</p>
  {#if restorable}
    <Button variant="outline" disabled={restoreDisabled} onclick={onrestore}
      >{m.common_draft_restore()}</Button
    >
  {:else}
    <p class="text-sm text-muted-foreground">{m.common_draft_read_only()}</p>
  {/if}
  <Button variant="ghost" onclick={ondiscard}>{m.common_draft_discard()}</Button>
</div>
