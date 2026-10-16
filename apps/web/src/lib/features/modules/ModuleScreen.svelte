<script lang="ts">
  // 모듈 하나: 항목 목록과 항목 만들기, 관리자의 이름·키 변경과 보관 (D-15, D-16, D-18).
  import type { Module, Workspace } from '@fvoci/api-client';

  import type { ItemView } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
  import { forgetReferences } from '$lib/features/references/resolver';
  import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
  import { itemPath, router } from '$lib/router.svelte';
  import { Badge } from '$lib/ui/badge';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  import { KIND_LABEL, normalizeKey } from './kinds';

  interface Props {
    workspace: Workspace;
    moduleId: string;
  }

  const { workspace, moduleId }: Props = $props();

  let mod = $state<Module | undefined>(undefined);
  let items = $state<ItemView[]>([]);
  let error = $state<string | undefined>(undefined);

  $effect(() => {
    const id = moduleId;
    let cancelled = false;
    mod = undefined;
    error = undefined;
    Promise.all([api.module(id), api.items(id)])
      .then(([m, list]) => {
        if (cancelled) return;
        mod = m;
        items = list;
        editName = m.name;
        editKey = m.key;
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        error =
          e instanceof ApiError && e.status === 404
            ? '모듈이 없거나 볼 권한이 없습니다.'
            : '모듈을 불러오지 못했습니다.';
      });
    return () => {
      cancelled = true;
    };
  });

  const isAdmin = $derived(workspace.role === 'admin');
  const canCreate = $derived(workspace.role !== 'reader' && mod?.archived === false);

  let editName = $state('');
  let editKey = $state('');
  let settingsError = $state<string | undefined>(undefined);
  let settingsNote = $state<string | undefined>(undefined);

  async function saveSettings(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (mod === undefined) return;
    const key = normalizeKey(editKey);
    if (editName.trim() === '' || key === undefined) {
      settingsError = '이름과 올바른 키(영문으로 시작하는 2~10자)를 입력하세요.';
      return;
    }
    settingsError = undefined;
    settingsNote = undefined;
    try {
      const before = mod.key;
      mod = await api.updateModule(mod.id, editName.trim(), key);
      forgetReferences();
      editKey = mod.key;
      items = await api.items(mod.id);
      if (before !== mod.key) {
        settingsNote = `키를 ${before}에서 ${mod.key}(으)로 바꿨습니다. 번호와 링크는 그대로이고, ${before}는 이 모듈의 예전 키로 남습니다.`;
      }
    } catch (e: unknown) {
      settingsError =
        e instanceof ApiError && e.status === 409
          ? '이미 쓰고 있거나 다른 모듈이 예전에 쓴 키입니다.'
          : e instanceof ApiError
            ? e.message
            : '바꾸지 못했습니다.';
    }
  }

  async function toggleArchive(): Promise<void> {
    if (mod === undefined) return;
    try {
      mod = await api.archiveModule(mod.id, !mod.archived);
    } catch (e: unknown) {
      settingsError = e instanceof ApiError ? e.message : '바꾸지 못했습니다.';
    }
  }
</script>

{#if error !== undefined}
  <p role="alert" class="text-destructive">{error}</p>
{:else if mod === undefined}
  <p aria-live="polite">불러오는 중…</p>
{:else}
  {@const current = mod}
  <section class="flex flex-col gap-6" aria-labelledby="module-heading">
    <div class="flex flex-wrap items-baseline gap-2">
      <h1 id="module-heading" class="text-xl font-semibold">{current.name}</h1>
      <span class="font-mono text-sm text-muted-foreground">{current.key}</span>
      <Badge variant="outline">{KIND_LABEL[current.kind]}</Badge>
      {#if current.archived}<Badge variant="secondary">보관됨 · 읽기 전용</Badge>{/if}
    </div>
    {#if current.previous_keys.length > 0}
      <p class="text-sm text-muted-foreground">예전 키: {current.previous_keys.join(', ')}</p>
    {/if}

    {#if items.length === 0}
      <p class="text-muted-foreground">볼 수 있는 항목이 없습니다.</p>
    {:else}
      <ul class="flex flex-col gap-1" data-testid="item-list">
        {#each items as it (it.id)}
          <li>
            <a
              class="flex items-baseline gap-2 rounded px-2 py-1.5 hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
              href={itemPath(workspace.id, it.id)}
              onclick={(e) => {
                e.preventDefault();
                router.navigate(itemPath(workspace.id, it.id));
              }}
            >
              <span class="font-mono text-sm text-muted-foreground">{it.reference}</span>
              <span>{it.title}</span>
              {#if it.visibility === 'private'}<Badge variant="outline">비공개</Badge>{/if}
              {#if it.archived}<Badge variant="secondary">보관됨</Badge>{/if}
            </a>
          </li>
        {/each}
      </ul>
    {/if}

    {#if canCreate}
      <CreateForm
        id="new-item"
        label="새 항목 제목"
        placeholder="예: 로그인 개선"
        submitLabel="항목 만들기"
        create={async (requestId: string, title: string) => {
          const created = await api.createItem(current.id, requestId, title);
          router.navigate(itemPath(workspace.id, created.id));
        }}
      />
      <p class="text-sm text-muted-foreground">
        새 항목은 비공개로 만들어집니다. 만든 사람과 관리자만 보며, 항목 화면에서 공개할 수
        있습니다.
      </p>
    {/if}

    {#if isAdmin}
      <form
        class="flex flex-wrap items-end gap-2 border-t pt-4"
        onsubmit={(e) => void saveSettings(e)}
        novalidate
        aria-label="모듈 설정"
      >
        <div class="flex min-w-40 flex-1 flex-col gap-1">
          <Label for="module-edit-name">이름</Label>
          <Input id="module-edit-name" bind:value={editName} maxlength={200} />
        </div>
        <div class="flex w-28 flex-col gap-1">
          <Label for="module-edit-key">키</Label>
          <Input
            id="module-edit-key"
            bind:value={editKey}
            maxlength={10}
            autocomplete="off"
            class="font-mono uppercase"
          />
        </div>
        <Button type="submit" variant="outline">저장</Button>
        <Button type="button" variant="outline" onclick={() => void toggleArchive()}
          >{current.archived ? '보관 해제' : '보관'}</Button
        >
      </form>
      {#if settingsNote !== undefined}
        <p class="text-sm" role="status">{settingsNote}</p>
      {/if}
      {#if settingsError !== undefined}
        <p class="text-sm text-destructive" role="alert">{settingsError}</p>
      {/if}
    {/if}
  </section>
{/if}
