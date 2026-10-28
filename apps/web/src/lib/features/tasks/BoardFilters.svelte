<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { TaskFilter, TaskSort } from './filters';
  import { EMPTY_FILTER, hiddenConditionCount, isFiltered } from './filters';
  import { PRIORITY_LABEL } from './labels';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { NativeSelect } from '$lib/ui/native-select';
  import * as Collapsible from '$lib/ui/collapsible';
  let {
    filtersOpen = $bindable(false),
    filter = $bindable({ ...EMPTY_FILTER }),
    sort = $bindable('position'),
    statusNames,
    people,
    shownCount,
    total,
    children,
  }: {
    filtersOpen?: boolean;
    filter?: TaskFilter;
    sort?: TaskSort;
    statusNames: string[];
    people: [string, string][];
    shownCount: number;
    total: number;
    children: Snippet;
  } = $props();
  const hiddenConditions = $derived(hiddenConditionCount(filter, sort));
  const SORT_LABEL: Record<TaskSort, string> = {
    get position() {
      return m.tasks_sort_position();
    },
    get due() {
      return m.tasks_sort_due();
    },
    get priority() {
      return m.tasks_sort_priority();
    },
    get title() {
      return m.tasks_sort_title();
    },
    get updated() {
      return m.tasks_sort_updated();
    },
  };
</script>

<Collapsible.Root
  bind:open={filtersOpen}
  class="flex flex-col gap-2 text-sm"
  role="search"
  aria-label={m.tasks_filter_region()}
>
  <div class="flex flex-wrap items-end gap-2">
    <label class="flex flex-col gap-0.5">
      <span class="text-xs text-muted-foreground">{m.tasks_filter_find()}</span>
      <Input
        class="w-40"
        type="search"
        bind:value={filter.text}
        placeholder={m.tasks_filter_find_placeholder()}
      />
    </label>
    <Collapsible.Trigger>
      {#snippet child({ props })}
        <Button {...props} variant="outline" data-testid="task-filter-toggle">
          {m.tasks_filter_toggle()}
          {#if hiddenConditions > 0}
            <span
              class="rounded-full bg-primary px-1.5 text-xs text-primary-foreground tabular-nums"
              aria-hidden="true">{hiddenConditions}</span
            >
            <span class="sr-only">{m.tasks_filter_active({ count: hiddenConditions })}</span>
          {/if}
        </Button>
      {/snippet}
    </Collapsible.Trigger>
    {#if isFiltered(filter) || sort !== 'position'}
      <Button
        variant="ghost"
        onclick={() => {
          filter = { ...EMPTY_FILTER };
          sort = 'position';
        }}>{m.tasks_filter_reset()}</Button
      >
    {/if}
    {#if isFiltered(filter)}
      <span class="self-center text-xs text-muted-foreground" aria-live="polite"
        >{m.tasks_filter_shown({ shown: shownCount, total: total })}</span
      >
    {/if}
  </div>
  <Collapsible.Content class="flex flex-col gap-2">
    <div class="flex flex-wrap items-end gap-2">
      <label class="flex flex-col gap-0.5">
        <span class="text-xs text-muted-foreground">{m.tasks_status()}</span>
        <NativeSelect
          class="w-full"
          bind:value={filter.status}
          aria-label={m.tasks_filter_status_aria()}
        >
          <option value="any">{m.tasks_filter_all()}</option>
          {#if filter.status !== 'any' && !statusNames.includes(filter.status)}
            <option value={filter.status}>{filter.status}</option>
          {/if}
          {#each statusNames as s (s)}<option value={s}>{s}</option>{/each}
        </NativeSelect>
      </label>
      <label class="flex flex-col gap-0.5">
        <span class="text-xs text-muted-foreground">{m.tasks_assignee()}</span>
        <NativeSelect
          class="w-full"
          bind:value={filter.assignee}
          aria-label={m.tasks_filter_assignee_aria()}
        >
          <option value="any">{m.tasks_filter_all()}</option>
          <option value="me">{m.tasks_filter_mine()}</option>
          <option value="none">{m.tasks_filter_unassigned()}</option>
          {#each people as [id, name] (id)}<option value={id}>{name}</option>{/each}
        </NativeSelect>
      </label>
      <label class="flex flex-col gap-0.5">
        <span class="text-xs text-muted-foreground">{m.tasks_priority()}</span>
        <NativeSelect
          class="w-full"
          bind:value={filter.priority}
          aria-label={m.tasks_filter_priority_aria()}
        >
          <option value="any">{m.tasks_filter_all()}</option>
          {#each ['urgent', 'high', 'medium', 'low', 'none'] as const as p (p)}<option value={p}
              >{PRIORITY_LABEL[p]}</option
            >{/each}
        </NativeSelect>
      </label>
      <label class="flex flex-col gap-0.5">
        <span class="text-xs text-muted-foreground">{m.tasks_tag()}</span>
        <Input class="w-24" bind:value={filter.tag} aria-label={m.tasks_filter_tag_aria()} />
      </label>
      <label class="flex flex-col gap-0.5">
        <span class="text-xs text-muted-foreground">{m.tasks_filter_due_from()}</span>
        <Input
          class="w-auto"
          type="date"
          bind:value={filter.dueFrom}
          aria-label={m.tasks_filter_due_from_aria()}
        />
      </label>
      <label class="flex flex-col gap-0.5">
        <span class="text-xs text-muted-foreground">{m.tasks_filter_due_to()}</span>
        <Input
          class="w-auto"
          type="date"
          bind:value={filter.dueTo}
          aria-label={m.tasks_filter_due_to_aria()}
        />
      </label>
      <label class="flex flex-col gap-0.5">
        <span class="text-xs text-muted-foreground">{m.tasks_sort()}</span>
        <NativeSelect class="w-full" bind:value={sort} aria-label={m.tasks_sort()}>
          {#each Object.entries(SORT_LABEL) as [value, label] (value)}<option {value}
              >{label}</option
            >{/each}
        </NativeSelect>
      </label>
    </div>
    {@render children()}
  </Collapsible.Content>
</Collapsible.Root>
