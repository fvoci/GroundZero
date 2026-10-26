<script lang="ts">
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  interface Props {
    onrestore: () => void;
    ondiscard: () => void;
  }
  const { onrestore, ondiscard }: Props = $props();
</script>

<div class="flex flex-wrap items-center gap-2 rounded border p-3" data-testid="form-draft-offer">
  <p class="text-sm">{m.common_draft_available()}</p>
  <Button variant="outline" onclick={onrestore}>{m.common_draft_restore()}</Button>
  <Button variant="ghost" onclick={ondiscard}>{m.common_draft_discard()}</Button>
</div>
