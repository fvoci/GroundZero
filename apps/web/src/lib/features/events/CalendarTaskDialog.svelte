<script lang="ts">
  // 달력의 날짜에서 업무 만들기 (D-31·D-80). 만들 대상(항목)을 분명히 한다: 항목 범위면 그 항목, 모듈 범위면 그 모듈의
  // 항목 가운데, 워크스페이스 범위면 모듈과 항목을 고른다(쓸 수 있는 것만: 보관·서식이 아니고 읽기 전용이 아닌 항목).
  // 저장은 기존 흐름 그대로다: 업무 만들기(`createTask`, 같은 입력의 재시도는 같은 요청 id) 뒤 그 업무에 마감일을 넣는다
  // (`updateTask`). 날짜를 넣지 못하면 업무는 이미 있으므로 다시 만들지 않고 날짜만 다시 넣게 한다.
  import type { Module } from '@fvoci/api-client';
  import { untrack } from 'svelte';

  import type { ItemView, TaskDetailView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { RetryKey } from '$lib/api/retry-key';
  import { formOf } from '$lib/features/tasks/form';
  import { m } from '$lib/paraglide/messages.js';
  import { itemTaskPath, router } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';
  import { Input } from '$lib/ui/input';
  import { NativeSelect } from '$lib/ui/native-select';

  interface Props {
    workspaceId: string;
    /** 처음 고른 날짜(YYYY-MM-DD). undefined면 창을 닫는다. */
    day: string | undefined;
    /** 항목 범위: 이 항목에 만든다. */
    item?: { id: string; title: string } | undefined;
    /** 모듈 범위: 이 모듈의 항목 가운데 고른다. */
    moduleId?: string | undefined;
    /** 창을 닫는다. `created`는 업무는 만들었지만 마감일을 넣지 못한 채 닫았는지(부르는 쪽이 목록을 다시 읽게). */
    onclose: (created: boolean) => void;
    oncreated: (task: TaskDetailView) => void;
  }

  const { workspaceId, day, item, moduleId, onclose, oncreated }: Props = $props();

  const writable = (i: ItemView): boolean => i.access !== 'read' && !i.archived && !i.is_template;

  let date = $state('');
  let title = $state('');
  let pickedModule = $state('');
  let pickedItem = $state('');
  let modules = $state<Module[] | undefined>(undefined);
  let items = $state<ItemView[] | undefined>(undefined);
  let loadError = $state<string | undefined>(undefined);
  let busy = $state(false);
  let error = $state<string | undefined>(undefined);
  // 만들었지만 날짜를 넣지 못한 업무(다시 시도는 날짜만).
  let made = $state<TaskDetailView | undefined>(undefined);
  const key = new RetryKey();

  // 창을 열 때마다 처음부터.
  $effect(() => {
    if (day === undefined) return;
    date = day;
    title = '';
    error = undefined;
    made = undefined;
    pickedModule = moduleId ?? '';
    pickedItem = item?.id ?? '';
    key.done();
  });

  // 워크스페이스 범위: 모듈 목록.
  $effect(() => {
    if (day === undefined || item !== undefined || moduleId !== undefined) return;
    let cancelled = false;
    modules = undefined;
    api
      .modules(workspaceId)
      .then((list) => {
        if (!cancelled) modules = list.filter((mod) => !mod.archived);
      })
      .catch((e: unknown) => {
        if (!cancelled) loadError = errorText(e, m.events_task_targets_failed());
      });
    return () => {
      cancelled = true;
    };
  });
  // 고른 모듈의 쓸 수 있는 항목.
  $effect(() => {
    const mod = pickedModule;
    if (day === undefined || item !== undefined) return;
    items = undefined;
    loadError = undefined;
    // 다른 모듈을 고르면 앞 모듈의 항목 선택은 버린다(새 목록을 읽는 사이 앞 항목에 만들지 않게).
    untrack(() => {
      pickedItem = '';
    });
    if (mod === '') return;
    let cancelled = false;
    api
      .items(mod)
      .then((list) => {
        if (cancelled) return;
        items = list.filter(writable);
        if (!items.some((i) => i.id === pickedItem)) pickedItem = items[0]?.id ?? '';
      })
      .catch((e: unknown) => {
        if (!cancelled) loadError = errorText(e, m.events_task_targets_failed());
      });
    return () => {
      cancelled = true;
    };
  });

  // 만들 항목: 고정 항목이거나, 지금 읽은 목록에 있는 고른 항목일 때만(목록을 읽는 중이거나 없으면 비어 있다).
  const targetId = $derived(
    item?.id ?? ((items ?? []).some((i) => i.id === pickedItem) ? pickedItem : ''),
  );
  /** 닫기. 보내는 중에는 닫지 않는다(늦게 온 결과가 다시 연 창의 날짜로 저장되지 않게). */
  function close(): void {
    if (busy) return;
    onclose(made !== undefined);
  }

  async function putDate(task: TaskDetailView, due: string): Promise<void> {
    const result = await api.updateTask(
      task.id,
      task.version,
      { ...formOf(task), due_date: due },
      [],
    );
    if (result.kind !== 'updated') {
      // 막 만든 업무를 그사이 누가 고쳤다. 지금 판으로 다시 넣을 수 있게 둔다.
      made = result.current;
      error = m.events_task_date_failed();
      return;
    }
    made = undefined;
    oncreated(result.task);
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (busy) return;
    error = undefined;
    const name = title.trim();
    if (made === undefined && (name === '' || targetId === '')) {
      error = name === '' ? m.events_task_title_required() : m.events_task_target_required();
      return;
    }
    if (date === '') {
      error = m.events_task_date_required();
      return;
    }
    // 보낼 때의 날짜로 저장한다(보내는 사이 입력이 바뀌어도).
    const due = date;
    busy = true;
    try {
      if (made !== undefined) {
        // 업무는 이미 있다. 날짜만 다시 넣는다(최신 판으로).
        await putDate(await api.task(made.id), due);
        return;
      }
      const requestId = key.for({ item: targetId, title: name });
      const created = await api.createTask(targetId, requestId, name);
      key.done(requestId);
      made = created;
      try {
        await putDate(created, due);
      } catch (e: unknown) {
        error = `${m.events_task_date_failed()} ${errorText(e, '')}`.trim();
      }
    } catch (e: unknown) {
      error = errorText(
        e,
        made === undefined ? m.events_task_create_failed() : m.events_task_date_failed(),
      );
    } finally {
      busy = false;
    }
  }
</script>

<Dialog.Root
  open={day !== undefined}
  onOpenChange={(o: boolean) => {
    if (!o) close();
  }}
>
  <Dialog.Content
    class="sm:max-w-md"
    escapeKeydownBehavior={busy ? 'ignore' : 'close'}
    interactOutsideBehavior={busy ? 'ignore' : 'close'}
    showCloseButton={!busy}
    onCloseAutoFocus={(e: Event) => {
      // 닫히면 부르는 쪽이 그 날짜의 만들기 단추로 초점을 돌린다.
      e.preventDefault();
    }}
  >
    <Dialog.Header>
      <Dialog.Title>{m.events_task_create_title()}</Dialog.Title>
      <Dialog.Description>{m.events_task_create_description()}</Dialog.Description>
    </Dialog.Header>
    <form class="flex flex-col gap-3" onsubmit={(e) => void submit(e)} novalidate>
      {#if item !== undefined}
        <p class="text-sm">{m.events_task_in_item({ title: item.title })}</p>
      {:else}
        {#if moduleId === undefined}
          <label class="flex flex-col gap-1 text-sm">
            {m.events_scope_module()}
            <NativeSelect class="w-full" bind:value={pickedModule} disabled={made !== undefined}>
              <option value="">{m.events_task_pick_module()}</option>
              {#each modules ?? [] as mod (mod.id)}
                <option value={mod.id}>{mod.name} ({mod.key})</option>
              {/each}
            </NativeSelect>
          </label>
        {/if}
        <label class="flex flex-col gap-1 text-sm">
          {m.events_scope_item()}
          <NativeSelect
            class="w-full"
            bind:value={pickedItem}
            disabled={made !== undefined || items === undefined || items.length === 0}
          >
            {#each items ?? [] as it (it.id)}
              <option value={it.id}>{it.title}</option>
            {/each}
          </NativeSelect>
        </label>
        {#if pickedModule !== '' && items?.length === 0}
          <p class="text-sm text-muted-foreground">{m.events_task_no_items()}</p>
        {:else if pickedModule !== '' && items === undefined && loadError === undefined}
          <p class="text-sm text-muted-foreground" role="status">{m.modules_loading()}</p>
        {/if}
        {#if loadError !== undefined}
          <p class="text-sm text-destructive" role="alert">{loadError}</p>
        {/if}
      {/if}
      <label class="flex flex-col gap-1 text-sm">
        {m.events_task_due()}
        <Input type="date" bind:value={date} required disabled={busy} />
      </label>
      <label class="flex flex-col gap-1 text-sm">
        {m.events_task_name()}
        <Input
          bind:value={title}
          required
          disabled={busy || made !== undefined}
          autocomplete="off"
        />
      </label>
      {#if error !== undefined}
        <div role="alert" class="flex flex-col gap-1 text-sm text-destructive">
          <p>{error}</p>
          {#if made !== undefined}
            {@const task = made}
            <a
              class="underline"
              href={itemTaskPath(workspaceId, task.item_id, task.id)}
              onclick={(e: MouseEvent) => {
                e.preventDefault();
                close();
                router.navigate(itemTaskPath(workspaceId, task.item_id, task.id));
              }}>{m.events_task_open({ title: task.title })}</a
            >
          {/if}
        </div>
      {/if}
      <Dialog.Footer>
        <Button type="button" variant="outline" disabled={busy} onclick={close}
          >{m.events_task_cancel()}</Button
        >
        <Button type="submit" disabled={busy}>
          {made === undefined ? m.events_task_create() : m.events_task_retry_date()}
        </Button>
      </Dialog.Footer>
    </form>
  </Dialog.Content>
</Dialog.Root>
