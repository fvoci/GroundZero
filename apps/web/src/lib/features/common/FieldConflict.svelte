<script lang="ts" generics="T">
  import { tick } from 'svelte';
  import { Button } from '$lib/ui/button';
  import { m } from '$lib/paraglide/messages.js';
  import { fieldDifferences, type ConflictField, type FieldSide } from './field-conflict';
  let {
    fields,
    base,
    mine,
    server,
    choices = $bindable({}),
    busy = false,
    blocked = false,
    repeated = false,
    onsave,
    onedit,
    oncancel,
    onoriginal,
  }: {
    fields: readonly ConflictField<T>[];
    base: T;
    mine: T;
    server: T;
    choices?: Record<string, FieldSide>;
    busy?: boolean;
    blocked?: boolean;
    repeated?: boolean;
    onsave: () => void;
    onedit: () => void;
    oncancel: () => void;
    onoriginal?: (() => void) | undefined;
  } = $props();
  const rows = $derived(fieldDifferences(fields, base, mine, server));
  const id = $props.id();
  const sides = ['mine', 'server'] as const;
  const actionClass = 'h-auto min-w-0 max-w-full whitespace-normal break-words';
  const selectable = $derived(rows.filter((row) => row.kind !== 'server'));
  const serverOnly = $derived(rows.filter((row) => row.kind === 'server'));
  let missing = $state(0);
  let heading = $state<HTMLElement>();
  let panel = $state<HTMLElement>();
  $effect(() => {
    if (!busy) heading?.focus();
  });
  $effect(() => {
    const element = panel;
    if (element === undefined) return;
    const escape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      event.stopPropagation();
      if (!busy) oncancel();
    };
    element.addEventListener('keydown', escape);
    return () => {
      element.removeEventListener('keydown', escape);
    };
  });
  async function proceed(action: () => void) {
    const unresolved = selectable.filter((row) => choices[row.field.key] === undefined);
    missing = unresolved.length;
    if (unresolved.length > 0) {
      await tick();
      const first = unresolved[0];
      if (first !== undefined)
        panel
          ?.querySelector<HTMLInputElement>(`input[name="${id}-choice-${first.field.key}"]`)
          ?.focus();
      return;
    }
    action();
  }
</script>

<section
  class="@container flex min-w-0 flex-col gap-3 rounded-md border bg-card p-3 text-card-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
  aria-labelledby={`${id}-heading`}
  tabindex="-1"
  bind:this={panel}
>
  <h3
    id={`${id}-heading`}
    class="font-semibold outline-none"
    tabindex="-1"
    aria-describedby={`${id}-hint`}
    bind:this={heading}
  >
    {m.common_conflict_heading()}
  </h3>
  <p id={`${id}-hint`}>
    {repeated ? m.common_conflict_repeated() : m.common_conflict_hint()}
  </p>
  {#if blocked}<p role="alert">{m.common_conflict_blocked()}</p>{/if}
  {#if missing > 0}<p role="alert">{m.common_conflict_missing({ count: missing })}</p>{/if}
  {#each selectable as row (row.field.key)}
    {@const field = row.field}
    <fieldset
      class="min-w-0 rounded border p-2"
      disabled={busy}
      aria-describedby={`${id}-${field.key}-hint`}
    >
      <legend class="max-w-full font-medium break-words">{field.label}</legend>
      <p id={`${id}-${field.key}-hint`} class="text-sm text-muted-foreground">
        {row.kind === 'both' ? m.common_conflict_both() : m.common_conflict_mine_only()}
      </p>
      <div class="grid min-w-0 gap-3 @lg:grid-cols-2" data-conflict-pair={field.key}>
        {#each sides as side (side)}
          <div class="min-w-0">
            <label
              class="flex items-center gap-2 rounded p-1 focus-within:ring-2 focus-within:ring-ring"
            >
              <input
                type="radio"
                class="size-4 shrink-0 appearance-none rounded-full border border-input-border bg-card checked:border-primary checked:bg-primary"
                name={`${id}-choice-${field.key}`}
                value={side}
                disabled={blocked}
                checked={choices[field.key] === side}
                aria-label={`${field.label}: ${side === 'mine' ? m.common_conflict_use_mine() : m.common_conflict_use_server()}`}
                onchange={() => {
                  choices = { ...choices, [field.key]: side };
                  missing = 0;
                }}
              />
              <span
                >{side === 'mine'
                  ? m.common_conflict_use_mine()
                  : m.common_conflict_use_server()}</span
              >
            </label>
            <textarea
              readonly
              class="min-h-20 max-h-60 w-full resize-y rounded border border-input-border bg-muted p-2 font-sans text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
              aria-label={`${field.label}: ${side === 'mine' ? m.common_conflict_mine() : m.common_conflict_server()}`}
              data-conflict-value={side}
              value={field.text(side === 'mine' ? mine : server)}></textarea>
          </div>
        {/each}
      </div>
      {#if field.text(mine).replace(/\p{Cf}|\s/gu, '') === field
          .text(server)
          .replace(/\p{Cf}|\s/gu, '')}
        <p class="text-sm text-muted-foreground">{m.common_conflict_looks_same()}</p>
      {/if}
      <details>
        <summary>{m.common_conflict_base()}</summary>
        <p class="break-words whitespace-pre-wrap">{field.text(base)}</p>
      </details>
    </fieldset>
  {/each}
  {#if serverOnly.length > 0}
    <details>
      <summary>{m.common_conflict_server_changes({ count: serverOnly.length })}</summary>
      <dl class="flex min-w-0 flex-col gap-2">
        {#each serverOnly as { field } (field.key)}
          <div>
            <dt class="font-medium">{field.label}</dt>
            <dd class="break-words whitespace-pre-wrap">{field.text(server)}</dd>
          </div>
        {/each}
      </dl>
    </details>
  {/if}
  {#if rows.length === 0}<p>{m.common_conflict_equal()}</p>{/if}
  <div class="flex flex-wrap gap-2">
    <Button class={actionClass} disabled={busy || blocked} onclick={() => void proceed(onsave)}
      >{m.common_conflict_save()}</Button
    >
    <Button
      class={actionClass}
      variant="outline"
      disabled={busy || blocked}
      onclick={() => void proceed(onedit)}>{m.common_conflict_edit()}</Button
    >
    <Button class={actionClass} variant="ghost" disabled={busy} onclick={oncancel}
      >{m.common_conflict_cancel()}</Button
    >
    {#if onoriginal !== undefined}
      <Button class={actionClass} variant="ghost" disabled={busy} onclick={onoriginal}
        >{m.common_conflict_original()}</Button
      >
    {/if}
  </div>
</section>
