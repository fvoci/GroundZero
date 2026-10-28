<script lang="ts">
  import { taskData as work } from '$lib/features/tasks/detail-data';
  // 달력의 날짜에서 업무 만들기 (D-31·D-80). 만들 대상(항목)을 분명히 한다: 항목 범위면 그 항목, 모듈 범위면 그 모듈의
  // 항목 가운데, 워크스페이스 범위면 모듈과 항목을 고른다(쓸 수 있는 것만: 보관·서식이 아니고 읽기 전용이 아닌 항목).
  // 제목·마감은 한 번의 생성 요청으로 저장한다(#104).
  import type { Module } from '@fvoci/api-client';
  import { onMount, untrack } from 'svelte';

  import type { ItemView, TaskDetailView } from '$lib/api/client';
  import { api, ApiError, newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { object, request } from './input-drafts';
  import { m } from '$lib/paraglide/messages.js';
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
    /** 생성은 원자적이므로 부분 생성 상태는 없다. */
    onclose: (created: boolean) => void;
    oncreated: (task: TaskDetailView) => void;
  }

  const { workspaceId, day, item, moduleId, onclose, oncreated }: Props = $props();
  let activeDay = $state(untrack(() => day));

  const writable = (i: ItemView): boolean => i.access !== 'read' && !i.archived && !i.is_template;

  let accountBlocked = $state(false);
  let date = $state(untrack(() => activeDay ?? ''));
  let title = $state('');
  let pickedModule = $state(untrack(() => moduleId ?? ''));
  let pickedItem = $state(untrack(() => item?.id ?? ''));
  let modules = $state<Module[] | undefined>(undefined);
  let items = $state<ItemView[] | undefined>(undefined);
  let loadError = $state<string | undefined>(undefined);
  let busy = $state(false);
  let error = $state<string | undefined>(undefined);
  let retry = $state<{ id: string; payload: string } | null>(null);
  interface Draft {
    date: string;
    title: string;
    module: string;
    item: string;
    retry: typeof retry;
  }
  // 항목 목록을 읽으며 자동으로 고른 항목. 사용자가 고친 것이 아니므로 수정 판단의 기준값이다.
  let autoItem = $state('');
  function reset(): void {
    date = activeDay ?? '';
    title = '';
    // 모듈이 그대로면 목록을 다시 읽지 않으므로 자동으로 고른 항목으로 돌린다.
    if (pickedModule !== (moduleId ?? '')) autoItem = '';
    pickedModule = moduleId ?? '';
    pickedItem = item?.id ?? autoItem;
    retry = null;
    error = undefined;
    accountBlocked = false;
    busy = false;
  }
  const draft = createFormDraft<Draft>({
    name: () =>
      activeDay === undefined
        ? undefined
        : `calendar-task:${workspaceId}:${item?.id ?? moduleId ?? 'workspace'}:${activeDay}`,
    snapshot: () => ({ date, title, module: pickedModule, item: pickedItem, retry }),
    parse: (value) => {
      const o = object(value);
      if (!o) return;
      const retry = request(o.retry);
      if (
        typeof o.date !== 'string' ||
        typeof o.title !== 'string' ||
        typeof o.module !== 'string' ||
        typeof o.item !== 'string' ||
        retry === undefined
      )
        return;
      return { date: o.date, title: o.title, module: o.module, item: o.item, retry };
    },
    restore: (value) => {
      date = value.date;
      title = value.title;
      pickedModule = value.module;
      pickedItem = value.item;
      retry = value.retry;
    },
    reset,
    dirty: () =>
      title !== '' ||
      date !== (activeDay ?? '') ||
      pickedModule !== (moduleId ?? '') ||
      pickedItem !== (item?.id ?? autoItem),
    busy: () => busy,
  });
  onMount(draft.attach);
  $effect(() => {
    const next = day;
    untrack(() => {
      if (next !== activeDay) {
        draft.leave.keep();
        draft.leave.request(() => {
          activeDay = next;
        });
      }
    });
  });

  // 워크스페이스 범위: 모듈 목록.
  $effect(() => {
    if (activeDay === undefined || item !== undefined || moduleId !== undefined) return;
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
    if (activeDay === undefined || item !== undefined) return;
    items = undefined;
    loadError = undefined;
    if (mod === '') return;
    let cancelled = false;
    api
      .items(mod)
      .then((list) => {
        if (cancelled) return;
        items = list.filter(writable);
        autoItem = items[0]?.id ?? '';
        if (!items.some((i) => i.id === pickedItem)) pickedItem = autoItem;
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
    draft.leave.request(() => {
      onclose(false);
    });
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (busy || accountBlocked) return;
    const attempt = draft.capture();
    if (attempt === undefined) return;
    error = undefined;
    const name = title.trim();
    if (name === '' || targetId === '') {
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
      const payload = JSON.stringify({ item: targetId, title: name, due });
      if (retry?.payload !== payload) retry = { id: newRequestId(), payload };
      const saving = draft.capture();
      if (saving === undefined) return;
      const created = await work.createTask(attempt.userId, targetId, retry.id, name, null, {
        due_date: due,
      });
      if (!draft.complete(saving)) return;
      oncreated(created);
    } catch (e: unknown) {
      if (!draft.isOwner(attempt)) return;
      if (e instanceof ApiError && e.code === 'account_changed') accountBlocked = true;
      error = errorText(e, m.events_task_create_failed());
    } finally {
      if (draft.isOwner(attempt)) busy = false;
    }
  }
</script>

<Dialog.Root
  open={activeDay !== undefined}
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
    {#if accountBlocked || !draft.active}<p role="alert">
        {m.tasks_account_changed()}
      </p>{/if}
    <form
      hidden={accountBlocked || !draft.active}
      class="flex flex-col gap-3"
      onsubmit={(e) => void submit(e)}
      novalidate
    >
      <!-- 저장하는 동안 초안을 되살리면 생긴 업무를 버리고 새 요청 ID로 하나 더 만들었다(D2-F-1). 입력과 함께 잠근다. -->
      {#each draft.offers as offer (offer.key)}
        <fieldset class="contents" disabled={busy}>
          <FormDraftOffer
            onrestore={() => {
              draft.restore(offer);
            }}
            ondiscard={() => {
              draft.discard(offer);
            }}
          />
        </fieldset>
      {/each}
      {#if item !== undefined}
        <p class="text-sm">{m.events_task_in_item({ title: item.title })}</p>
      {:else}
        {#if moduleId === undefined}
          <label class="flex flex-col gap-1 text-sm">
            {m.events_scope_module()}
            <NativeSelect class="w-full" bind:value={pickedModule} disabled={busy}>
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
            disabled={busy || items === undefined || items.length === 0}
          >
            {#each items ?? [] as it (it.id)}
              <option value={it.id}>{it.title}</option>
            {/each}
          </NativeSelect>
        </label>
        {#if pickedModule !== '' && items?.length === 0}
          <p class="text-sm text-muted-foreground">{m.events_task_no_items()}</p>
        {/if}
        <p class="text-sm text-muted-foreground empty:hidden" role="status">
          {#if pickedModule !== '' && items === undefined && loadError === undefined}{m.modules_loading()}{/if}
        </p>
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
        <Input bind:value={title} required disabled={busy} autocomplete="off" />
      </label>
      {#if error !== undefined}
        <div role="alert" class="flex flex-col gap-1 text-sm text-destructive">
          <p>{error}</p>
        </div>
      {/if}
      <Dialog.Footer>
        <Button type="button" variant="outline" disabled={busy} onclick={close}
          >{m.events_task_cancel()}</Button
        >
        <Button type="submit" disabled={busy}>
          {m.events_task_create()}
        </Button>
      </Dialog.Footer>
    </form>
    <DiscardChangesConfirm
      open={draft.leave.open}
      onkeep={draft.leave.keep}
      ondiscard={draft.discardAndLeave}
    />
  </Dialog.Content>
</Dialog.Root>
