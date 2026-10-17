<script lang="ts">
  // 모듈 하나: 설명, 항목 목록과 항목 만들기, 관리자의 이름·키·아이콘·설명 변경과 보관·휴지통 (D-15, D-16, D-18, D-78).
  import type { Module, Workspace } from '@fvoci/api-client';

  import type { ItemView } from '$lib/api/client';
  import { api, ApiError, newRequestId } from '$lib/api/client';
  import { forgetReferences } from '$lib/features/references/resolver';
  import CalendarScreen from '$lib/features/events/CalendarScreen.svelte';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import DeleteButton from '$lib/features/trash/DeleteButton.svelte';
  import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
  import { itemPath, router, workspacePath } from '$lib/router.svelte';
  import { Badge } from '$lib/ui/badge';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  import { KIND_LABEL, normalizeKey } from './kinds';
  import { ITEM_STATUS_LABEL } from './profile';

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
        editIcon = m.icon;
        editDescription = m.description;
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
  let editIcon = $state('');
  let editDescription = $state('');
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
      if (editIcon.trim() !== mod.icon) await api.setModuleIcon(mod.id, editIcon.trim());
      if (editDescription.trim() !== mod.description) {
        await api.setModuleDescription(mod.id, editDescription.trim());
      }
      mod = await api.updateModule(mod.id, editName.trim(), key);
      forgetReferences();
      editKey = mod.key;
      items = await api.items(mod.id);
      notifyDone(m.common_saved());
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

  // 템플릿에서 만들기(D-33): 새로 시작(상태·날짜·체크 비우기)으로 복제한다.
  const templates = $derived(items.filter((i) => i.is_template && !i.archived));
  let templateId = $state('');
  let templateTitle = $state('');
  let templateError = $state<string | undefined>(undefined);
  let templateRequest = newRequestId();

  async function fromTemplate(moduleId: string): Promise<void> {
    const source = templateId === '' ? templates[0]?.id : templateId;
    if (source === undefined || templateTitle.trim() === '') {
      templateError = '템플릿과 새 항목 제목을 고르세요.';
      return;
    }
    templateError = undefined;
    try {
      const copied = await api.copyItem(
        source,
        templateRequest,
        moduleId,
        templateTitle.trim(),
        true,
      );
      templateRequest = newRequestId();
      router.navigate(itemPath(workspace.id, copied.item_id));
    } catch (e: unknown) {
      templateError = e instanceof ApiError ? e.message : '만들지 못했습니다.';
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
      <h1 id="module-heading" class="text-xl font-semibold">
        {#if current.icon !== ''}<span aria-hidden="true">{current.icon}</span>{/if}
        {current.name}
      </h1>
      <span class="font-mono text-sm text-muted-foreground">{current.key}</span>
      <Badge variant="outline">{KIND_LABEL[current.kind]}</Badge>
      {#if current.archived}<Badge variant="secondary">보관됨 · 읽기 전용</Badge>{/if}
      {#if isAdmin}
        <div class="ml-auto">
          <DeleteButton
            what="모듈"
            remove={() => api.trashModule(current.id)}
            ondeleted={() => {
              router.navigate(workspacePath(workspace.id));
            }}
          />
        </div>
      {/if}
    </div>
    {#if current.description !== ''}
      <p class="whitespace-pre-wrap">{current.description}</p>
    {/if}
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
              <Badge variant="outline">{ITEM_STATUS_LABEL[it.status]}</Badge>
              {#if it.starts_on !== null || it.ends_on !== null}
                <span class="text-sm text-muted-foreground"
                  >{it.starts_on ?? ''} ~ {it.ends_on ?? ''}</span
                >
              {/if}
              {#if it.visibility === 'private'}<Badge variant="outline">비공개</Badge>{/if}
              {#if it.archived}<Badge variant="secondary">보관됨</Badge>{/if}
              {#if it.is_template}<Badge variant="secondary">템플릿</Badge>{/if}
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
      {#if templates.length > 0}
        <form
          class="flex flex-wrap items-end gap-2"
          aria-label="템플릿에서 만들기"
          onsubmit={(e: SubmitEvent) => {
            e.preventDefault();
            void fromTemplate(current.id);
          }}
        >
          <div class="flex flex-col gap-1">
            <Label for="template-source">템플릿</Label>
            <select
              id="template-source"
              class="h-9 rounded-md border border-input bg-transparent px-3 text-base"
              bind:value={templateId}
            >
              {#each templates as t (t.id)}<option value={t.id}>{t.title}</option>{/each}
            </select>
          </div>
          <div class="flex min-w-40 flex-1 flex-col gap-1">
            <Label for="template-title">템플릿으로 만들 항목 제목</Label>
            <Input id="template-title" bind:value={templateTitle} maxlength={200} />
          </div>
          <Button type="submit" variant="outline">템플릿에서 만들기</Button>
        </form>
        {#if templateError !== undefined}
          <p class="text-sm text-destructive" role="alert">{templateError}</p>
        {/if}
      {/if}
    {/if}

    <CalendarScreen {workspace} scope={{ moduleId: current.id }} embedded />

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
        <div class="flex w-24 flex-col gap-1">
          <Label for="module-edit-icon">아이콘</Label>
          <Input id="module-edit-icon" bind:value={editIcon} maxlength={16} placeholder="예: 🚀" />
        </div>
        <div class="flex w-full flex-col gap-1">
          <Label for="module-edit-description">설명</Label>
          <textarea
            id="module-edit-description"
            bind:value={editDescription}
            maxlength={2000}
            rows={3}
            class="rounded-md border border-input bg-transparent px-3 py-2 text-base shadow-xs focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          ></textarea>
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
