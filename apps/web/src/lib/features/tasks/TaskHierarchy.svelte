<script lang="ts">
  import type { TaskDetailView, TaskView } from './model';
  import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import * as Select from '$lib/ui/select';
  interface Props {
    current: TaskDetailView;
    canEdit: boolean;
    subtasks: TaskView[];
    parentCandidates: TaskView[];
    byId: Map<string, TaskView>;
    label: (task: TaskView | undefined) => string;
    onopen: (id: string) => void;
    changeParent: (id: string, parentId: string | null) => Promise<void>;
    parentChoice?: string;
    childSeed: number;
    childInitial: { name: string; requestId: string | null } | undefined;
    onchilddraft: (draft: { name: string; requestId: string | null } | undefined) => void;
    createChild: (requestId: string, title: string) => Promise<void>;
  }
  let {
    current,
    canEdit,
    subtasks,
    parentCandidates,
    byId,
    label,
    onopen,
    changeParent,
    parentChoice = $bindable(''),
    childSeed,
    childInitial,
    onchilddraft,
    createChild,
  }: Props = $props();
  const TOP = 'top';
</script>

<section class="flex flex-col gap-2" aria-labelledby="parent-heading">
  <h3 id="parent-heading" class="text-sm font-semibold">{m.tasks_parent()}</h3>
  {#if current.parent_id !== null}
    {@const parentId = current.parent_id}
    <button
      type="button"
      class="self-start rounded px-2 py-1 text-left text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
      onclick={() => {
        onopen(parentId);
      }}>{label(byId.get(parentId)) || m.tasks_hidden_task()}</button
    >
  {:else}
    <p class="text-sm text-muted-foreground">{m.tasks_parent_top()}</p>
  {/if}
  {#if canEdit && subtasks.length > 0}
    <p class="text-sm text-muted-foreground">{m.tasks_parent_has_subtasks()}</p>
  {:else if canEdit && (parentCandidates.length > 0 || current.parent_id !== null)}
    <div class="flex flex-wrap items-center gap-2">
      <span id="parent-choose-label" class="sr-only">{m.tasks_parent_choose()}</span>
      <Select.Root
        type="single"
        value={parentChoice}
        onValueChange={(v: string) => (parentChoice = v)}
      >
        <Select.Trigger aria-labelledby="parent-choose-label"
          >{parentChoice === ''
            ? m.tasks_parent_choose()
            : parentChoice === TOP
              ? m.tasks_parent_top()
              : label(byId.get(parentChoice))}</Select.Trigger
        >
        <Select.Content>
          {#if current.parent_id !== null}
            <Select.Item value={TOP} label={m.tasks_parent_top()}
              >{m.tasks_parent_top()}</Select.Item
            >
          {/if}
          {#each parentCandidates as c (c.id)}
            <Select.Item value={c.id} label={label(c)}>{label(c)}</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
      <Button
        size="sm"
        disabled={parentChoice === ''}
        onclick={() => {
          const chosen = parentChoice;
          void changeParent(current.id, chosen === TOP ? null : chosen);
        }}>{m.tasks_parent_change()}</Button
      >
    </div>
  {/if}
</section>

{#if current.parent_id === null}
  <section class="flex flex-col gap-2" aria-labelledby="subtasks-heading">
    <h3 id="subtasks-heading" class="text-sm font-semibold">{m.tasks_subtasks()}</h3>
    {#each subtasks as sub (sub.id)}
      <button
        type="button"
        class="flex items-baseline gap-2 rounded px-2 py-1 text-left text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        onclick={() => {
          onopen(sub.id);
        }}
      >
        <span class="shrink-0 font-mono text-xs whitespace-nowrap text-muted-foreground"
          >{sub.reference ?? ''}</span
        >
        <span class="min-w-0 [overflow-wrap:anywhere]">{sub.title}</span>
        <span class="ml-auto shrink-0 text-xs whitespace-nowrap text-muted-foreground"
          >{sub.status}</span
        >
      </button>
    {:else}
      <p class="text-sm text-muted-foreground">{m.tasks_none()}</p>
    {/each}
    {#if canEdit}
      {#key childSeed}
        <CreateForm
          id="new-subtask"
          label={m.tasks_subtask_title()}
          placeholder={m.tasks_subtask_placeholder()}
          submitLabel={m.tasks_subtask_add()}
          initial={childInitial}
          onchange={(draft: { name: string; requestId: string | null } | undefined) => {
            onchilddraft(draft);
          }}
          create={createChild}
        />
      {/key}
    {/if}
  </section>
{/if}
