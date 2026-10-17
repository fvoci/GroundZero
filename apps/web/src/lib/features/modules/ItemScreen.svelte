<script lang="ts">
  // 항목 하나: 제목·설명·공개 범위, 보관, 항목 아래 문서 (D-14, D-20, D-59).
  import MarkButtons from '$lib/features/home/MarkButtons.svelte';
  import DeleteButton from '$lib/features/trash/DeleteButton.svelte';
  import type { Module, Workspace } from '@fvoci/api-client';

  import type { ItemView, SummaryView } from '$lib/api/client';
  import { api, ApiError, newRequestId } from '$lib/api/client';
  import AttachmentsPanel from '$lib/features/files/AttachmentsPanel.svelte';
  import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
  import DocumentTree from '$lib/features/documents/DocumentTree.svelte';
  import type { TreeMove } from '$lib/features/documents/tree';
  import { forgetReferences } from '$lib/features/references/resolver';
  import Backlinks from '$lib/features/references/Backlinks.svelte';
  import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
  import ItemEvents from '$lib/features/events/ItemEvents.svelte';
  import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
  import { itemDocumentPath, modulePath, router } from '$lib/router.svelte';
  import { Badge } from '$lib/ui/badge';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  import ItemProfilePanel from './ItemProfilePanel.svelte';

  interface Props {
    workspace: Workspace;
    itemId: string;
    /** 주소에서 연 업무(상세 대화상자). */
    taskId?: string | undefined;
    workspaces: Workspace[];
  }

  const { workspace, itemId, workspaces, taskId }: Props = $props();
  // 주소 객체가 바뀌어도(업무 대화상자 열고 닫기) 같은 항목이면 다시 불러오지 않는다.
  const currentItemId = $derived(itemId);

  let item = $state<ItemView | undefined>(undefined);
  let mod = $state<Module | undefined>(undefined);
  let docs = $state<SummaryView[]>([]);
  let error = $state<string | undefined>(undefined);

  let title = $state('');
  let description = $state('');
  let shared = $state(false);
  let formError = $state<string | undefined>(undefined);
  let saved = $state(false);

  function fill(next: ItemView): void {
    item = next;
    title = next.title;
    description = next.description;
    shared = next.visibility === 'workspace';
  }

  $effect(() => {
    const id = currentItemId;
    let cancelled = false;
    item = undefined;
    error = undefined;
    api
      .item(id)
      .then(async (it) => {
        const [m, list] = await Promise.all([api.module(it.module_id), api.itemDocuments(id)]);
        if (cancelled) return;
        fill(it);
        mod = m;
        docs = list;
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        error =
          e instanceof ApiError && e.status === 404
            ? '항목이 없거나 볼 권한이 없습니다.'
            : '항목을 불러오지 못했습니다.';
      });
    return () => {
      cancelled = true;
    };
  });

  const canEdit = $derived(item !== undefined && !item.archived && item.access !== 'read');
  const isOwner = $derived(item?.access === 'owner');

  async function save(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (item === undefined) return;
    if (title.trim() === '') {
      formError = '제목을 입력하세요.';
      return;
    }
    formError = undefined;
    saved = false;
    try {
      fill(
        await api.updateItem(item.id, {
          title: title.trim(),
          description,
          visibility: shared ? 'workspace' : 'private',
        }),
      );
      forgetReferences();
      saved = true;
    } catch (e: unknown) {
      formError = e instanceof ApiError ? e.message : '저장하지 못했습니다.';
    }
  }

  async function toggleArchive(): Promise<void> {
    if (item === undefined) return;
    try {
      fill(await api.archiveItem(item.id, !item.archived));
    } catch (e: unknown) {
      formError = e instanceof ApiError ? e.message : '바꾸지 못했습니다.';
    }
  }
</script>

{#if error !== undefined}
  <p role="alert" class="text-destructive">{error}</p>
{:else if item === undefined}
  <p aria-live="polite">불러오는 중…</p>
{:else}
  {@const current = item}
  <section class="flex flex-col gap-6" aria-labelledby="item-heading">
    {#if mod !== undefined}
      {@const parent = mod}
      <nav aria-label="경로" class="text-sm">
        <a
          class="underline"
          href={modulePath(workspace.id, parent.id)}
          onclick={(e: MouseEvent) => {
            e.preventDefault();
            router.navigate(modulePath(workspace.id, parent.id));
          }}>{parent.name}</a
        >
      </nav>
    {/if}
    <div class="flex flex-wrap items-baseline gap-2">
      <span class="font-mono text-sm text-muted-foreground">{current.reference}</span>
      <h1 id="item-heading" class="text-xl font-semibold">{current.title}</h1>
      <Badge variant="outline"
        >{current.visibility === 'private' ? '비공개' : '워크스페이스 공개'}</Badge
      >
      {#if current.archived}<Badge variant="secondary">보관됨 · 읽기 전용</Badge>{/if}
      <div class="ml-auto flex flex-wrap items-center gap-1">
        <MarkButtons kind="item" id={current.id} />
        {#if isOwner}
          <DeleteButton
            what="항목"
            remove={() => api.trashItem(current.id)}
            ondeleted={() => {
              router.navigate(modulePath(workspace.id, current.module_id));
            }}
          />
        {/if}
      </div>
    </div>

    {#if canEdit}
      <form
        class="flex flex-col gap-3"
        onsubmit={(e) => void save(e)}
        novalidate
        aria-label="항목 정보"
      >
        <div class="flex flex-col gap-1">
          <Label for="item-title">제목</Label>
          <Input id="item-title" bind:value={title} maxlength={200} />
        </div>
        <div class="flex flex-col gap-1">
          <Label for="item-description">설명</Label>
          <textarea
            id="item-description"
            bind:value={description}
            maxlength={2000}
            rows={3}
            class="rounded-md border border-input bg-transparent px-3 py-2 text-base shadow-xs focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          ></textarea>
        </div>
        {#if isOwner}
          <div class="flex items-center gap-2">
            <Checkbox id="item-shared" bind:checked={shared} />
            <Label for="item-shared">워크스페이스 멤버에게 공개(독자는 읽기, 작성자는 고치기)</Label
            >
          </div>
        {/if}
        <div class="flex gap-2">
          <Button type="submit">저장</Button>
          {#if isOwner}
            <Button type="button" variant="outline" onclick={() => void toggleArchive()}
              >보관</Button
            >
          {/if}
        </div>
      </form>
    {:else}
      {#if current.description !== ''}
        <p class="whitespace-pre-wrap">{current.description}</p>
      {/if}
      {#if current.archived && isOwner}
        <div>
          <Button variant="outline" onclick={() => void toggleArchive()}>보관 해제</Button>
        </div>
      {/if}
    {/if}
    {#if saved}<p class="text-sm" role="status">저장했습니다.</p>{/if}
    {#if formError !== undefined}
      <p class="text-sm text-destructive" role="alert">{formError}</p>
    {/if}

    {#if mod !== undefined}
      <ItemProfilePanel
        item={current}
        kind={mod.kind}
        onchanged={() => {
          api
            .item(current.id)
            .then(fill)
            .catch(() => undefined);
        }}
      />
    {/if}

    {#if mod?.kind === 'course'}
      <CoursePanel itemId={current.id} />
    {/if}

    <AttachmentsPanel target={{ kind: 'item', id: current.id }} canWrite={canEdit} />

    <section class="flex flex-col gap-3" aria-labelledby="item-docs-heading">
      <h2 id="item-docs-heading" class="text-lg font-semibold">문서</h2>
      {#if docs.length === 0}
        <p class="text-muted-foreground">문서가 없습니다.</p>
      {:else}
        <DocumentTree
          {docs}
          label={`${current.title} 문서`}
          hrefFor={(id: string) => itemDocumentPath(workspace.id, current.id, id)}
          navigate={(path: string) => {
            router.navigate(path);
          }}
          canWrite={canEdit}
          onmove={async (id: string, move: TreeMove) => {
            await api.setParent(id, move.parentId, move.beforeId);
            docs = await api.itemDocuments(current.id);
          }}
          oncreatechild={async (parentId: string) => {
            const doc = await api.createItemDocument(
              current.id,
              newRequestId(),
              '새 문서',
              parentId,
            );
            router.navigate(itemDocumentPath(workspace.id, current.id, doc.id));
          }}
        />
      {/if}
      {#if canEdit}
        <CreateForm
          id="new-item-document"
          label="새 문서 제목"
          placeholder="예: 설계 메모"
          submitLabel="문서 만들기"
          create={async (requestId: string, docTitle: string) => {
            const doc = await api.createItemDocument(current.id, requestId, docTitle);
            router.navigate(itemDocumentPath(workspace.id, current.id, doc.id));
          }}
        />
      {/if}
    </section>
    <TaskBoard item={current} {workspaces} openTaskId={taskId} />
    <ItemEvents item={current} />
    <Backlinks kind="item" id={current.id} {workspaces} />
  </section>
{/if}
