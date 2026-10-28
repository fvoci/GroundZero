<script lang="ts">
  import type { TaskDetailView, RecurrenceRule } from './model';
  import RecurrenceEditor from '$lib/features/events/RecurrenceEditor.svelte';
  import { localTimeZone } from '$lib/features/events/time';
  import { Button } from '$lib/ui/button';
  import { m } from '$lib/paraglide/messages.js';
  const {
    current,
    latestRule,
    canEdit,
    onopen,
    saveRule,
    onunsaved,
    ondraftkept,
    onsaving,
  }: {
    current: TaskDetailView;
    latestRule: TaskDetailView | undefined;
    canEdit: boolean;
    onopen: (id: string) => void;
    saveRule: (rule: RecurrenceRule | null, base?: RecurrenceRule | null) => Promise<void>;
    onunsaved: (value: boolean) => void;
    ondraftkept: (value: boolean) => void;
    onsaving: (value: boolean) => void;
  } = $props();
  let editor = $state<{ discardDraft: () => void; isSaving: () => boolean }>();
  export function discardDraft(): void {
    editor?.discardDraft();
  }
  export function isSaving(): boolean {
    return editor?.isSaving() === true;
  }
</script>

{#if current.parent_id === null}
  {#if current.series !== null && current.series.root_task_id !== current.id}
    {@const rootId = current.series.root_task_id}
    <section class="flex flex-col gap-2" aria-labelledby="task-recurrence-heading">
      <h3 id="task-recurrence-heading" class="text-sm font-semibold">
        {m.tasks_recurrence()}
      </h3>
      <p class="text-sm text-muted-foreground">
        {m.tasks_occurrence_note({ date: current.occurrence_date ?? '' })}
      </p>
      <div>
        <Button
          size="sm"
          variant="outline"
          onclick={() => {
            onopen(rootId);
          }}>{m.tasks_open_first()}</Button
        >
      </div>
    </section>
  {:else}
    {#key current.id}
      <RecurrenceEditor
        bind:this={editor}
        idPrefix="task-recurrence"
        draftName={`task-recurrence:${current.id}`}
        rule={(latestRule ?? current).series?.rule ?? null}
        canEdit={canEdit && (current.due_date !== null || current.start_date !== null)}
        zoneNote={m.tasks_recurrence_zone({
          zone: current.series?.timezone ?? localTimeZone(),
        })}
        onsave={saveRule}
        onunsaved={(value: boolean) => {
          onunsaved(value);
        }}
        ondraftkept={(value: boolean) => {
          ondraftkept(value);
        }}
        onsaving={(value: boolean) => {
          onsaving(value);
        }}
      />
    {/key}
    {#if canEdit && current.due_date === null && current.start_date === null}
      <p class="text-sm text-muted-foreground">{m.tasks_recurrence_needs_date()}</p>
    {/if}
  {/if}
{/if}
