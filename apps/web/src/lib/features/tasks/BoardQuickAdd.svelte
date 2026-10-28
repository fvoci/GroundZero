<script lang="ts">
  import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
  import { Button } from '$lib/ui/button';
  import { m } from '$lib/paraglide/messages.js';
  const {
    lane,
    adding,
    initial,
    create,
    cancel,
    onchange,
    onopen,
  }: {
    lane: string;
    adding: boolean;
    initial: { name: string; requestId: string | null } | undefined;
    create: (requestId: string, title: string) => Promise<void>;
    cancel: () => Promise<void>;
    onchange: (draft: { name: string; requestId: string | null } | undefined) => void;
    onopen: () => void;
  } = $props();
</script>

{#if adding}
  <CreateForm
    id={`new-task-${lane}`}
    label={m.tasks_quick_add_label({ status: lane })}
    placeholder={m.tasks_new_placeholder()}
    submitLabel={m.tasks_create()}
    create={(requestId: string, title: string) => create(requestId, title)}
    autofocus
    oncancel={() => void cancel()}
    {initial}
    onchange={(draft: { name: string; requestId: string | null } | undefined) => {
      onchange(draft);
    }}
  />
{:else}
  <Button size="sm" variant="ghost" class="self-start" data-quick-add={lane} onclick={onopen}
    >{m.tasks_quick_add()}</Button
  >
{/if}
