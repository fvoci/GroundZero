<script lang="ts">
  const editingUser = untrack(() => me.userId);
  import { onMount, untrack } from 'svelte';
  import { me } from '$lib/me.svelte';
  import type { StatusCategory, TaskStatusDefinition } from '@fvoci/api-client';
  import { api, ApiError } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { NativeSelect } from '$lib/ui/native-select';
  import * as Dialog from '$lib/ui/dialog';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';

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
  // 범주는 고정값 셋이라 번역한다(D-116). 상태 이름은 사용자가 정한 그대로 보인다.
  const categoryLabel: Record<StatusCategory, () => string> = {
    todo: m.tasks_category_todo,
    doing: m.tasks_category_doing,
    done: m.tasks_category_done,
  };
  let position = $state('');
  let moveTo = $state('');
  let removing = $state(false);
  let busy = $state(false);
  let error = $state('');
  let invalid = $state('');
  interface StatusDraft {
    name: string;
    color: string;
    category: StatusCategory;
    position: string;
  }
  const initial = (): StatusDraft => ({
    name: selected?.name ?? '',
    color: selected?.color ?? '',
    category: selected?.category ?? 'todo',
    position: String(selected?.position ?? statuses.length),
  });
  const snapshot = (): StatusDraft => ({ name, color, category, position });
  const draft = createFormDraft<StatusDraft>({
    name: () => (open && !removing ? `task-status:${itemId}:${selected?.id ?? 'new'}` : undefined),
    snapshot,
    parse: (value) => {
      if (
        typeof value !== 'object' ||
        value === null ||
        !('name' in value && 'color' in value && 'category' in value && 'position' in value)
      )
        return undefined;
      if (
        typeof value.name !== 'string' ||
        typeof value.color !== 'string' ||
        typeof value.position !== 'string' ||
        (value.category !== 'todo' && value.category !== 'doing' && value.category !== 'done')
      )
        return undefined;
      return {
        name: value.name,
        color: value.color,
        position: value.position,
        category: value.category,
      };
    },
    restore: (value) => {
      name = value.name;
      color = value.color;
      category = value.category;
      position = value.position;
    },
    reset: () => {
      const value = initial();
      name = value.name;
      color = value.color;
      category = value.category;
      position = value.position;
    },
    dirty: () => open && !removing && JSON.stringify(snapshot()) !== JSON.stringify(initial()),
    busy: () => busy,
  });
  onMount(draft.attach);

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
        // 빈 순서는 Number('')가 0이라 통과해 버리고 열을 맨 앞으로 옮겼다(CodeRabbit). 비우면 거절한다.
        position.trim() === '' ||
        !Number.isFinite(Number(position)))
    ) {
      invalid = m.tasks_status_invalid();
      return;
    }
    if (removing && (selected === undefined || moveTo === '')) {
      invalid = m.tasks_status_move_required();
      return;
    }
    const attempt = draft.capture();
    if (!removing && attempt === undefined) return;
    busy = true;
    try {
      if (removing && selected !== undefined)
        await api.deleteTaskStatus(editingUser, itemId, selected.id, moveTo);
      else if (selected === undefined)
        await api.createTaskStatus(
          attempt?.userId ?? editingUser,
          itemId,
          name.trim(),
          color.toLowerCase(),
          category,
        );
      else
        await api.updateTaskStatus(attempt?.userId ?? editingUser, itemId, selected.id, {
          name: name.trim(),
          color: color.toLowerCase(),
          category,
          position: Number(position),
        });
      if (attempt !== undefined && !draft.isCurrent(attempt)) return;
      if (attempt !== undefined) draft.complete(attempt);
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
      <span class="text-sm text-muted-foreground"
        >{categoryLabel[status.category]()} · {status.position}</span
      >
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

<Dialog.Root
  bind:open={
    () => open,
    (value) => {
      if (!value)
        draft.leave.request(() => {
          open = false;
        });
    }
  }
>
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
      {#each draft.offers as entry (entry.key)}
        <FormDraftOffer
          onrestore={() => {
            draft.restore(entry);
          }}
          ondiscard={() => {
            draft.discard(entry);
          }}
        />
      {/each}
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
              <option value="todo">{categoryLabel.todo()}</option>
              <option value="doing">{categoryLabel.doing()}</option>
              <option value="done">{categoryLabel.done()}</option>
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
          <Button
            type="button"
            variant="outline"
            onclick={() => {
              draft.leave.request(() => {
                open = false;
              });
            }}>{m.common_cancel()}</Button
          >
          <Button type="submit" variant={removing ? 'destructive' : 'default'}
            >{removing ? m.tasks_status_delete_confirm() : m.common_save()}</Button
          >
        </Dialog.Footer>
      </fieldset>
    </form>
  </Dialog.Content>
</Dialog.Root>

<DiscardChangesConfirm
  open={draft.leave.open}
  onkeep={draft.leave.keep}
  ondiscard={draft.discardAndLeave}
/>
