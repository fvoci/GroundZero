<script lang="ts">
  import type { StatusCategory, TaskStatusDefinition } from '@fvoci/api-client';
  import { api, ApiError } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { NativeSelect } from '$lib/ui/native-select';
  import * as Dialog from '$lib/ui/dialog';

  const {
    itemId,
    statuses,
    archived,
    onchanged,
  }: {
    itemId: string;
    statuses: TaskStatusDefinition[];
    archived: boolean;
    onchanged: () => Promise<void>;
  } = $props();
  let open = $state(false);
  let selected = $state<TaskStatusDefinition | undefined>();
  let name = $state('');
  let color = $state('');
  let category = $state<StatusCategory>('todo');
  let position = $state('');
  let moveTo = $state('');
  let removing = $state(false);
  let busy = $state(false);
  let error = $state('');
  let invalid = $state('');

  function edit(status?: TaskStatusDefinition, remove = false): void {
    selected = status;
    name = status?.name ?? '';
    color = status?.color ?? '';
    category = status?.category ?? 'todo';
    position = String(status?.position ?? statuses.length);
    moveTo = '';
    error = '';
    invalid = '';
    removing = remove;
    open = true;
  }
  async function save(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (busy || archived) return;
    invalid = '';
    error = '';
    if (
      !removing &&
      (name.trim() === '' ||
        (color !== '' && !/^#[0-9a-fA-F]{6}$/.test(color)) ||
        !Number.isFinite(Number(position)))
    ) {
      invalid = m.tasks_status_invalid();
      return;
    }
    if (removing && (selected === undefined || moveTo === '')) {
      invalid = m.tasks_status_move_required();
      return;
    }
    busy = true;
    try {
      if (removing && selected !== undefined)
        await api.deleteTaskStatus(itemId, selected.id, moveTo);
      else if (selected === undefined)
        await api.createTaskStatus(itemId, name.trim(), color.toLowerCase(), category);
      else
        await api.updateTaskStatus(itemId, selected.id, {
          name: name.trim(),
          color: color.toLowerCase(),
          category,
          position: Number(position),
        });
      await onchanged();
      open = false;
      notifyDone(m.common_saved());
    } catch (e: unknown) {
      error =
        e instanceof ApiError && (e.status === 400 || e.status === 409)
          ? `${m.tasks_status_failed()} ${e.message}`
          : errorText(e, m.tasks_status_failed());
    } finally {
      busy = false;
    }
  }
</script>

<section class="flex flex-col gap-2 rounded-lg border p-3" aria-label={m.tasks_status_edit()}>
  {#each statuses as status (status.id)}
    <div class="flex flex-wrap items-center gap-2">
      <span class="font-medium">{status.name}</span>
      <span class="text-sm text-muted-foreground">{status.category} · {status.position}</span>
      <Button
        size="sm"
        variant="outline"
        disabled={archived}
        onclick={() => {
          edit(status);
        }}>{m.tasks_status_change({ name: status.name })}</Button
      >
      <Button
        size="sm"
        variant="outline"
        disabled={archived || statuses.length < 2}
        onclick={() => {
          edit(status, true);
        }}>{m.tasks_status_remove({ name: status.name })}</Button
      >
    </div>
  {/each}
  <Button
    variant="outline"
    disabled={archived}
    onclick={() => {
      edit();
    }}>{m.tasks_status_add()}</Button
  >
</section>

<Dialog.Root bind:open>
  <Dialog.Content
    escapeKeydownBehavior={busy ? 'ignore' : 'close'}
    interactOutsideBehavior={busy ? 'ignore' : 'close'}
    showCloseButton={!busy}
  >
    <Dialog.Header>
      <Dialog.Title
        >{removing ? m.tasks_status_remove({ name }) : m.tasks_status_edit()}</Dialog.Title
      >
      <Dialog.Description>{m.tasks_status_description()}</Dialog.Description>
    </Dialog.Header>
    <form class="flex flex-col gap-3" onsubmit={(e) => void save(e)}>
      <fieldset disabled={busy} class="flex flex-col gap-3">
        {#if removing}
          <label
            >{m.tasks_status_move_to()}
            <NativeSelect bind:value={moveTo}>
              <option value="">{m.tasks_status_move_required()}</option>
              {#each statuses.filter((s) => s.id !== selected?.id) as status (status.id)}
                <option value={status.id}>{status.name}</option>
              {/each}
            </NativeSelect>
          </label>
        {:else}
          <label>{m.tasks_status_name()}<Input bind:value={name} /></label>
          <label
            >{m.tasks_status_color()}<Input
              bind:value={color}
              placeholder="#rrggbb"
              aria-describedby="status-invalid"
            /></label
          >
          <label
            >{m.tasks_status_category()}
            <NativeSelect bind:value={category}>
              <option value="todo">{m.tasks_status_todo()}</option>
              <option value="doing">{m.tasks_status_in_progress()}</option>
              <option value="done">{m.tasks_status_done()}</option>
            </NativeSelect>
          </label>
          {#if selected !== undefined}<label
              >{m.tasks_status_position()}<Input bind:value={position} inputmode="decimal" /></label
            >{/if}
        {/if}
        <p id="status-invalid" role="status" class="text-sm text-destructive empty:hidden">
          {invalid}
        </p>
        {#if error}<p role="alert" class="text-sm text-destructive">{error}</p>{/if}
        <Dialog.Footer>
          <Button type="button" variant="outline" onclick={() => (open = false)}
            >{m.common_cancel()}</Button
          >
          <Button type="submit" variant={removing ? 'destructive' : 'default'}
            >{removing ? m.tasks_status_delete_confirm() : m.common_save()}</Button
          >
        </Dialog.Footer>
      </fieldset>
    </form>
  </Dialog.Content>
</Dialog.Root>
