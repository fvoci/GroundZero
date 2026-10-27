<script lang="ts">
  // 항목 복제(D-33): 같은 종류 모듈로 업무·하위 업무·체크리스트·선행 관계·문서 트리를 새 번호로 옮긴다. 담당자·반복
  // 규칙·기록·첨부·진도는 옮기지 않고, 복제를 허용하지 않은 남의 문서는 건너뛴다(D-20).
  import type { Module } from '@fvoci/api-client';
  import { onMount } from 'svelte';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { object, retry } from './draft-values';

  import type { ItemView } from '$lib/api/client';
  import { api, newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { itemPath, router } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import { NativeSelect } from '$lib/ui/native-select';

  interface Props {
    item: ItemView;
    module: Module;
    /** 앱 안 이동의 다음 경로에서도 이 대화상자가 마운트된 채인지(항목 화면 머리, D-121). 그러면 묻지 않는다. */
    stays?: ((next: string) => boolean) | undefined;
  }

  const { item, module, stays }: Props = $props();

  let open = $state(false);
  let targets = $state<Module[]>([]);
  let target = $state('');
  let title = $state('');
  let reset = $state(true);
  let error = $state<string | undefined>(undefined);
  // 같은 입력의 재시도는 같은 요청 id, 대상·제목·초기화를 고치면 새 id.
  let pending = $state<{ requestId: string; signature: string } | null>(null);
  let busy = $state(false);
  let initialTitle = $state('');
  let initialTarget = $state('');
  const draft = createFormDraft({
    name: () => `item-copy:${item.id}`,
    snapshot: () => ({ title, target, reset, pending }),
    parse: (raw) => {
      if (
        !object(raw) ||
        typeof raw.title !== 'string' ||
        typeof raw.target !== 'string' ||
        typeof raw.reset !== 'boolean' ||
        !retry(raw.pending)
      )
        return undefined;
      return { title: raw.title, target: raw.target, reset: raw.reset, pending: raw.pending };
    },
    restore: (value) => {
      title = value.title;
      target = value.target;
      reset = value.reset;
      pending = value.pending;
      open = true;
      void loadTargets();
    },
    reset: () => {
      title = '';
      target = '';
      initialTitle = '';
      initialTarget = '';
      reset = true;
      pending = null;
      open = false;
      busy = false;
      error = undefined;
    },
    dirty: () =>
      open && (pending !== null || title !== initialTitle || target !== initialTarget || !reset),
    busy: () => busy,
    stays: (next) => stays?.(next) === true,
  });
  onMount(draft.attach);
  async function loadTargets(): Promise<void> {
    const all = await api.modules(item.workspace_id).catch(() => []);
    targets = all.filter((mod) => mod.kind === module.kind && !mod.archived);
  }

  async function start(): Promise<void> {
    error = undefined;
    try {
      const all = await api.modules(item.workspace_id);
      targets = all.filter((mod) => mod.kind === module.kind && !mod.archived);
      target = module.archived ? (targets[0]?.id ?? '') : module.id;
      title = m.modules_copy_title_default({ title: item.title });
      initialTitle = title;
      initialTarget = target;
      pending = null;
      open = true;
    } catch (e: unknown) {
      error = errorText(e, m.modules_list_load_failed());
    }
  }

  async function submit(e: SubmitEvent): Promise<void> {
    e.preventDefault();
    if (busy || !draft.active || draft.blocked) return;
    if (target === '' || !targets.some((candidate) => candidate.id === target)) {
      error = m.modules_copy_pick_module();
      return;
    }
    error = undefined;
    let attempt: ReturnType<typeof draft.capture>;
    try {
      const name = title.trim();
      const signature = JSON.stringify({ item: item.id, target, name, reset });
      if (pending?.signature !== signature) pending = { requestId: newRequestId(), signature };
      const requestId = pending.requestId;
      attempt = draft.capture();
      if (attempt === undefined) return;
      busy = true;
      const copied = await api.copyItem(item.id, requestId, target, name, reset);
      if (!draft.complete(attempt)) return;
      open = false;
      // 일반 완료는 토스트다(D-82). 새 항목 열기는 토스트의 동작 단추이고, 건너뛴 문서 수는 문구에 넣는다.
      const created = copied.item_id;
      const skipped = copied.skipped_documents;
      notifyDone(
        skipped > 0
          ? `${m.modules_copy_done()} ${m.modules_copy_skipped({ count: skipped })}`
          : m.modules_copy_done(),
        `item-copy:${created}`,
        {
          action: {
            label: m.modules_copy_open(),
            onClick: () => {
              router.navigate(itemPath(item.workspace_id, created));
            },
          },
        },
      );
    } catch (err: unknown) {
      if (attempt === undefined || !draft.isCurrent(attempt)) return;
      error = errorText(err, m.modules_copy_failed());
    } finally {
      if (attempt !== undefined && draft.isOwner(attempt)) busy = false;
    }
  }
</script>

<!-- 보내는 동안 입력과 되살리기를 잠근다(D-97). 그사이 바뀐 입력은 성공한 복제를 버리게 하고, 다시 내면 새 요청 id로
     두 번째 사본을 만든다. -->
{#each draft.offers as entry (entry.key)}
  <fieldset class="contents" disabled={busy}>
    <FormDraftOffer
      onrestore={() => {
        draft.restore(entry);
      }}
      ondiscard={() => {
        draft.discard(entry);
      }}
    />
  </fieldset>
{/each}
<DiscardChangesConfirm
  open={draft.leave.open}
  onkeep={draft.leave.keep}
  ondiscard={draft.discardAndLeave}
/>

<div class="flex flex-col gap-2 text-sm">
  {#if !open}
    <div>
      <Button size="sm" variant="outline" onclick={() => void start()}>{m.modules_copy()}</Button>
    </div>
  {:else}
    <fieldset class="min-w-0" disabled={busy}>
      <form
        class="flex flex-wrap items-end gap-2 rounded-md border p-2"
        onsubmit={(e) => void submit(e)}
        aria-label={m.modules_copy_form()}
      >
        <div class="flex flex-col gap-1">
          <Label for="copy-target">{m.modules_copy_module()}</Label>
          <NativeSelect id="copy-target" class="w-full" bind:value={target}>
            {#each targets as mod (mod.id)}<option value={mod.id}>{mod.name} ({mod.key})</option
              >{/each}
          </NativeSelect>
        </div>
        <div class="flex min-w-[min(12rem,100%)] flex-1 flex-col gap-1">
          <Label for="copy-title">{m.modules_copy_new_title()}</Label>
          <Input id="copy-title" bind:value={title} maxlength={200} />
        </div>
        <div class="flex items-center gap-2 pb-2">
          <Checkbox id="copy-reset" bind:checked={reset} />
          <Label for="copy-reset">{m.modules_copy_reset()}</Label>
        </div>
        <Button type="submit" size="sm">{m.modules_copy()}</Button>
        <Button
          type="button"
          size="sm"
          variant="ghost"
          onclick={() => {
            draft.leave.request(() => (open = false));
          }}>{m.common_cancel()}</Button
        >
        <p class="w-full text-muted-foreground">{m.modules_copy_hint()}</p>
      </form>
    </fieldset>
  {/if}
  {#if error !== undefined}<p role="alert" class="text-destructive">{error}</p>{/if}
</div>
