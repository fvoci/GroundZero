<script lang="ts">
  // 항목 복제(D-33): 같은 종류 모듈로 업무·하위 업무·체크리스트·선행 관계·문서 트리를 새 번호로 옮긴다. 담당자·반복
  // 규칙·기록·첨부·진도는 옮기지 않고, 복제를 허용하지 않은 남의 문서는 건너뛴다(D-20).
  import type { Module } from '@fvoci/api-client';

  import type { ItemView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { RetryKey } from '$lib/api/retry-key';
  import { errorText } from '$lib/api/errors';
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
  }

  const { item, module }: Props = $props();

  let open = $state(false);
  let targets = $state<Module[]>([]);
  let target = $state('');
  let title = $state('');
  let reset = $state(true);
  let error = $state<string | undefined>(undefined);
  let result = $state<{ id: string; skipped: number } | undefined>(undefined);
  // 같은 입력의 재시도는 같은 요청 id, 대상·제목·초기화를 고치면 새 id.
  const copyKey = new RetryKey();

  async function start(): Promise<void> {
    error = undefined;
    result = undefined;
    try {
      const all = await api.modules(item.workspace_id);
      targets = all.filter((mod) => mod.kind === module.kind && !mod.archived);
      target = module.archived ? (targets[0]?.id ?? '') : module.id;
      title = m.modules_copy_title_default({ title: item.title });
      copyKey.done();
      open = true;
    } catch (e: unknown) {
      error = errorText(e, m.modules_list_load_failed());
    }
  }

  async function submit(e: SubmitEvent): Promise<void> {
    e.preventDefault();
    if (target === '') {
      error = m.modules_copy_pick_module();
      return;
    }
    error = undefined;
    try {
      const name = title.trim();
      const requestId = copyKey.for({ item: item.id, target, name, reset });
      const copied = await api.copyItem(item.id, requestId, target, name, reset);
      copyKey.done(requestId);
      result = { id: copied.item_id, skipped: copied.skipped_documents };
      open = false;
    } catch (err: unknown) {
      error = errorText(err, m.modules_copy_failed());
    }
  }
</script>

<div class="flex flex-col gap-2 text-sm">
  {#if !open}
    <div>
      <Button size="sm" variant="outline" onclick={() => void start()}>{m.modules_copy()}</Button>
    </div>
  {:else}
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
      <Button type="button" size="sm" variant="ghost" onclick={() => (open = false)}
        >{m.common_cancel()}</Button
      >
      <p class="w-full text-muted-foreground">{m.modules_copy_hint()}</p>
    </form>
  {/if}
  {#if error !== undefined}<p role="alert" class="text-destructive">{error}</p>{/if}
  <div role="status" class="empty:hidden">
    {#if result !== undefined}
      {@const done = result}
      <p>
        {m.modules_copy_done()}{done.skipped > 0
          ? ` ${m.modules_copy_skipped({ count: done.skipped })}`
          : ''}
        <a
          class="underline"
          href={itemPath(item.workspace_id, done.id)}
          onclick={(e: MouseEvent) => {
            e.preventDefault();
            router.navigate(itemPath(item.workspace_id, done.id));
          }}>{m.modules_copy_open()}</a
        >
      </p>
    {/if}
  </div>
</div>
