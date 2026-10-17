<script lang="ts">
  // 항목 복제(D-33): 같은 종류 모듈로 업무·하위 업무·체크리스트·선행 관계·문서 트리를 새 번호로 옮긴다. 담당자·반복
  // 규칙·기록·첨부·진도는 옮기지 않고, 복제를 허용하지 않은 남의 문서는 건너뛴다(D-20).
  import type { Module } from '@fvoci/api-client';

  import type { ItemView } from '$lib/api/client';
  import { api, ApiError, newRequestId } from '$lib/api/client';
  import { itemPath, router } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

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
  let requestId = newRequestId();

  async function start(): Promise<void> {
    error = undefined;
    result = undefined;
    try {
      const all = await api.modules(item.workspace_id);
      targets = all.filter((m) => m.kind === module.kind && !m.archived);
      target = module.archived ? (targets[0]?.id ?? '') : module.id;
      title = `${item.title} 사본`;
      requestId = newRequestId();
      open = true;
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '모듈 목록을 불러오지 못했습니다.';
    }
  }

  async function submit(e: SubmitEvent): Promise<void> {
    e.preventDefault();
    if (target === '') {
      error = '옮길 모듈을 고르세요.';
      return;
    }
    error = undefined;
    try {
      const copied = await api.copyItem(item.id, requestId, target, title.trim(), reset);
      requestId = newRequestId();
      result = { id: copied.item_id, skipped: copied.skipped_documents };
      open = false;
    } catch (err: unknown) {
      error = err instanceof ApiError ? err.message : '복제하지 못했습니다.';
    }
  }
</script>

<div class="flex flex-col gap-2 text-sm">
  {#if !open}
    <div><Button size="sm" variant="outline" onclick={() => void start()}>복제</Button></div>
  {:else}
    <form
      class="flex flex-wrap items-end gap-2 rounded-md border p-2"
      onsubmit={(e) => void submit(e)}
      aria-label="항목 복제"
    >
      <div class="flex flex-col gap-1">
        <Label for="copy-target">모듈</Label>
        <select
          id="copy-target"
          class="h-9 rounded-md border border-input bg-transparent px-3 text-base"
          bind:value={target}
        >
          {#each targets as m (m.id)}<option value={m.id}>{m.name} ({m.key})</option>{/each}
        </select>
      </div>
      <div class="flex min-w-48 flex-1 flex-col gap-1">
        <Label for="copy-title">새 제목</Label>
        <Input id="copy-title" bind:value={title} maxlength={200} />
      </div>
      <div class="flex items-center gap-2 pb-2">
        <Checkbox id="copy-reset" bind:checked={reset} />
        <Label for="copy-reset">새로 시작(상태·날짜·체크 비우기)</Label>
      </div>
      <Button type="submit" size="sm">복제</Button>
      <Button type="button" size="sm" variant="ghost" onclick={() => (open = false)}>취소</Button>
      <p class="w-full text-muted-foreground">
        업무·하위 업무·체크리스트·선행 관계·문서를 새 번호로 옮깁니다. 담당자·반복
        규칙·기록·첨부·진도는 옮기지 않고, 새 항목은 비공개입니다.
      </p>
    </form>
  {/if}
  {#if error !== undefined}<p role="alert" class="text-destructive">{error}</p>{/if}
  {#if result !== undefined}
    {@const done = result}
    <p role="status">
      복제했습니다.{done.skipped > 0
        ? ` 복제가 허용되지 않은 문서 ${String(done.skipped)}개는 건너뛰었습니다.`
        : ''}
      <a
        class="underline"
        href={itemPath(item.workspace_id, done.id)}
        onclick={(e: MouseEvent) => {
          e.preventDefault();
          router.navigate(itemPath(item.workspace_id, done.id));
        }}>새 항목 열기</a
      >
    </p>
  {/if}
</div>
