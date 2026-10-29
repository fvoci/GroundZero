<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { createFormDraft, type FormDraftAttempt } from '$lib/features/common/form-draft.svelte';

  const { filtered = false, retainSource = false }: { filtered?: boolean; retainSource?: boolean } =
    $props();
  let long = $state('');
  let short = $state('');
  let attempt = $state<FormDraftAttempt>();
  let completed = $state('');
  interface Input {
    long: string;
    short: string;
  }
  const options = {
    retainSource: untrack(() => retainSource),
    name: () => 'snapshot-test',
    snapshot: (): Input => ({ long, short }),
    ...(untrack(() => filtered) ? { storedSnapshot: (): Input => ({ long, short: '' }) } : {}),
    parse: (value: unknown): Input | undefined => {
      if (
        typeof value !== 'object' ||
        value === null ||
        !('long' in value) ||
        !('short' in value) ||
        typeof value.long !== 'string' ||
        typeof value.short !== 'string'
      )
        return undefined;
      return { long: value.long, short: value.short };
    },
    restore: (value: Input) => {
      long = value.long;
      short = value.short;
    },
    reset: () => {
      long = '';
      short = '';
    },
    dirty: () => long !== '' || short !== '',
  };
  const draft = createFormDraft(options);
  onMount(draft.attach);
</script>

<input aria-label="Long" bind:value={long} />
<input aria-label="Short" bind:value={short} />
<button
  type="button"
  onclick={() => {
    attempt = draft.capture();
  }}>Capture</button
>
<button
  type="button"
  onclick={() => {
    if (attempt !== undefined) completed = String(draft.complete(attempt));
  }}>Complete</button
>
<output aria-label="Completed">{completed}</output>
{#each draft.offers as entry (entry.key)}
  <button
    type="button"
    onclick={() => {
      draft.restore(entry);
    }}>Restore</button
  >
{/each}
