<script lang="ts">
  import type { AssigneeView, TaskInput, TaskPriority, TaskStatus } from './model';
  import { TASK_PRIORITIES } from './model';
  import { PRIORITY_LABEL } from './labels';
  import { me } from '$lib/me.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import { Textarea } from '$lib/ui/textarea';
  import * as Select from '$lib/ui/select';
  let {
    f = $bindable(),
    tagsText = $bindable(''),
    estimateText = $bindable(''),
    previewing,
    locked,
    canEdit,
    busy,
    statusNames,
    people,
    peopleError,
    peopleLabels,
    toggleAssignee,
    submit,
  }: {
    f: TaskInput | undefined;
    tagsText?: string;
    estimateText?: string;
    previewing: boolean;
    locked: boolean;
    canEdit: boolean;
    busy: boolean;
    statusNames: string[];
    people: AssigneeView[] | undefined;
    peopleError: string | undefined;
    peopleLabels: Map<string, string>;
    toggleAssignee: (id: string, checked: boolean) => void;
    submit: () => Promise<void>;
  } = $props();
</script>

{#if f !== undefined}
  <form
    class="grid grid-cols-1 gap-3 *:min-w-0 @md:grid-cols-2"
    inert={previewing}
    onsubmit={(e) => {
      e.preventDefault();
      void submit();
    }}
  >
    <div class="@md:col-span-2 flex flex-col gap-1">
      <Label for="task-title">{m.tasks_title()}</Label>
      <Input id="task-title" bind:value={f.title} disabled={locked} maxlength={me.maxTitleChars} />
    </div>
    <div class="flex min-w-0 flex-col gap-1">
      <span class="text-sm font-medium" id="task-status-label">{m.tasks_status()}</span>
      <Select.Root
        type="single"
        value={f.status}
        disabled={locked}
        onValueChange={(v: string) => {
          const s = statusNames.find((x) => x === v);
          if (s !== undefined) f.status = s satisfies TaskStatus;
        }}
      >
        <Select.Trigger aria-labelledby="task-status-label">{f.status}</Select.Trigger>
        <Select.Content>
          {#each statusNames as s (s)}
            <Select.Item value={s} label={s}>{s}</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
    </div>
    <div class="flex min-w-0 flex-col gap-1">
      <span class="text-sm font-medium" id="task-priority-label">{m.tasks_priority()}</span>
      <Select.Root
        type="single"
        value={f.priority}
        disabled={locked}
        onValueChange={(v: string) => {
          const p = TASK_PRIORITIES.find((x) => x === v);
          if (p !== undefined) f.priority = p satisfies TaskPriority;
        }}
      >
        <Select.Trigger aria-labelledby="task-priority-label"
          >{PRIORITY_LABEL[f.priority]}</Select.Trigger
        >
        <Select.Content>
          {#each TASK_PRIORITIES as p (p)}
            <Select.Item value={p} label={PRIORITY_LABEL[p]}>{PRIORITY_LABEL[p]}</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
    </div>
    <div class="flex min-w-0 flex-col gap-1">
      <Label for="task-start">{m.tasks_start_date()}</Label>
      <Input
        id="task-start"
        type="date"
        bind:value={f.start_date}
        disabled={locked || f.milestone}
      />
    </div>
    <div class="flex min-w-0 flex-col gap-1">
      <Label for="task-due">{m.tasks_due_date()}</Label>
      <Input id="task-due" type="date" bind:value={f.due_date} disabled={locked} />
    </div>
    <div class="@md:col-span-2 flex flex-col gap-1">
      <Label for="task-description">{m.tasks_description()}</Label>
      <Textarea
        id="task-description"
        bind:value={f.description}
        disabled={locked}
        rows={3}
        class="min-h-20"
      />
    </div>
    <div class="flex min-w-0 flex-col gap-1">
      <Label for="task-estimate">{m.tasks_estimate()}</Label>
      <Input id="task-estimate" inputmode="numeric" bind:value={estimateText} disabled={locked} />
    </div>
    <div class="flex min-w-0 flex-col gap-1">
      <Label for="task-tags">{m.tasks_tags()}</Label>
      <Input id="task-tags" bind:value={tagsText} disabled={locked} />
    </div>
    <div class="@md:col-span-2 flex items-center gap-2">
      <Checkbox
        id="task-milestone"
        checked={f.milestone}
        disabled={locked}
        onCheckedChange={(v) => {
          f.milestone = v === true;
          if (f.milestone) f.start_date = null;
        }}
      />
      <Label for="task-milestone">{m.tasks_milestone()}</Label>
    </div>
    <fieldset class="@md:col-span-2 flex flex-col gap-2">
      <legend class="text-sm font-medium">{m.tasks_assignees()}</legend>
      <div class="flex flex-wrap gap-3">
        {#if peopleError !== undefined}
          <span class="text-sm text-destructive" role="alert">{peopleError}</span>
        {:else if people === undefined}
          <span class="text-sm text-muted-foreground" role="status">{m.modules_loading()}</span>
        {:else}
          {#each people as person (person.user_id)}
            <span class="flex items-center gap-1.5">
              <Checkbox
                id={`assignee-${person.user_id}`}
                checked={f.assignee_ids.includes(person.user_id)}
                disabled={locked}
                onCheckedChange={(v) => {
                  toggleAssignee(person.user_id, v === true);
                }}
              />
              <Label for={`assignee-${person.user_id}`}
                >{peopleLabels.get(person.user_id) ?? person.display_name}</Label
              >
            </span>
          {:else}
            <span class="text-sm text-muted-foreground">{m.tasks_no_assignable()}</span>
          {/each}
        {/if}
      </div>
    </fieldset>
    {#if canEdit}
      <div class="@md:col-span-2 flex items-center gap-2">
        <Button id="task-save" type="submit" disabled={busy}>{m.common_save()}</Button>
      </div>
    {/if}
  </form>
{/if}
