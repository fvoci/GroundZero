<script lang="ts">
  // 항목 하나: 제목·설명·공개 범위, 보관, 항목 아래 문서 (D-14, D-20, D-59).
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import MarkButtons from '$lib/features/home/MarkButtons.svelte';
  import DeleteButton from '$lib/features/trash/DeleteButton.svelte';
  import type { Module, Workspace } from '@fvoci/api-client';

  import type { ItemView, SummaryView } from '$lib/api/client';
  import { api, ApiError, newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import AttachmentsPanel from '$lib/features/files/AttachmentsPanel.svelte';
  import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
  import DocumentTree from '$lib/features/documents/DocumentTree.svelte';
  import type { TreeMove } from '$lib/features/documents/tree';
  import { forgetReferences } from '$lib/features/references/resolver';
  import Backlinks from '$lib/features/references/Backlinks.svelte';
  import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
  import ItemEvents from '$lib/features/events/ItemEvents.svelte';
  import CalendarScreen from '$lib/features/events/CalendarScreen.svelte';
  import TemplatePicker from '$lib/features/documents/TemplatePicker.svelte';
  import RecordsPanel from '$lib/features/records/RecordsPanel.svelte';
  import StudyPanel from '$lib/features/study/StudyPanel.svelte';
  import ItemTimePanel from '$lib/features/time/ItemTimePanel.svelte';
  import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
  import { workspaceNav } from '$lib/features/nav/workspace-nav.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { itemDocumentPath, modulePath, router, workspacePath } from '$lib/router.svelte';
  import { Badge } from '$lib/ui/badge';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  import ItemCopy from './ItemCopy.svelte';
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
  let docTemplate = $state('');

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
        const [found, list] = await Promise.all([api.module(it.module_id), api.itemDocuments(id)]);
        if (cancelled) return;
        fill(it);
        mod = found;
        workspaceNav.currentModuleId = it.module_id;
        docs = list;
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        error =
          e instanceof ApiError && e.status === 404
            ? m.modules_item_not_found()
            : m.modules_item_load_failed();
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
      formError = m.modules_title_required();
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
      formError = errorText(e, m.modules_save_failed());
    }
  }

  async function toggleTemplate(): Promise<void> {
    if (item === undefined) return;
    try {
      await api.setItemTemplate(item.id, !item.is_template);
      fill(await api.item(item.id));
    } catch (e: unknown) {
      formError = errorText(e, m.modules_change_failed());
    }
  }

  async function toggleArchive(): Promise<void> {
    if (item === undefined) return;
    try {
      fill(await api.archiveItem(item.id, !item.archived));
    } catch (e: unknown) {
      formError = errorText(e, m.modules_change_failed());
    }
  }
</script>

{#if error !== undefined}
  <p role="alert" class="text-destructive">{error}</p>
{:else if item === undefined}
  <p aria-live="polite">{m.modules_loading()}</p>
{:else}
  {@const current = item}
  <section class="flex flex-col gap-6" aria-labelledby="item-heading">
    {#if mod !== undefined}
      {@const parent = mod}
      <nav aria-label={m.modules_breadcrumb()} class="-mb-4 min-w-0 text-sm text-muted-foreground">
        <ol class="flex min-w-0 items-center gap-1">
          {#each [{ path: workspacePath(workspace.id), label: workspace.name }, { path: modulePath(workspace.id, parent.id), label: `${parent.name} · ${parent.key}` }] as crumb, index (crumb.path)}
            <li class={`flex min-w-0 items-center gap-1 ${index === 0 ? 'shrink' : 'shrink-0'}`}>
              {#if index > 0}<ChevronRight class="size-3.5 shrink-0" aria-hidden="true" />{/if}
              <a
                class="truncate rounded-sm underline-offset-4 hover:text-foreground hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                href={crumb.path}
                title={crumb.label}
                onclick={(e: MouseEvent) => {
                  if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
                  e.preventDefault();
                  router.navigate(crumb.path);
                }}>{crumb.label}</a
              >
            </li>
          {/each}
        </ol>
      </nav>
    {/if}
    <div class="flex flex-wrap items-baseline gap-2">
      <span class="font-mono text-sm text-muted-foreground">{current.reference}</span>
      <h1 id="item-heading" class="text-xl font-semibold">{current.title}</h1>
      <Badge variant="outline"
        >{current.visibility === 'private'
          ? m.modules_private()
          : m.modules_shared_workspace()}</Badge
      >
      {#if current.archived}<Badge variant="secondary">{m.modules_archived_readonly()}</Badge>{/if}
      {#if current.is_template}<Badge variant="secondary">{m.modules_template()}</Badge>{/if}
      <div class="ml-auto flex flex-wrap items-center gap-1">
        <MarkButtons kind="item" id={current.id} />
        {#if isOwner}
          <DeleteButton
            what={m.modules_delete_item_what()}
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
        aria-label={m.modules_item_info()}
      >
        <div class="flex flex-col gap-1">
          <Label for="item-title">{m.modules_title()}</Label>
          <Input id="item-title" bind:value={title} maxlength={200} />
        </div>
        <div class="flex flex-col gap-1">
          <Label for="item-description">{m.modules_description()}</Label>
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
            <Label for="item-shared">{m.modules_share_with_members()}</Label>
          </div>
        {/if}
        <div class="flex gap-2">
          <Button type="submit">{m.common_save()}</Button>
          {#if isOwner}
            <Button type="button" variant="outline" onclick={() => void toggleArchive()}
              >{m.modules_archive()}</Button
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
          <Button variant="outline" onclick={() => void toggleArchive()}
            >{m.modules_unarchive()}</Button
          >
        </div>
      {/if}
    {/if}
    {#if saved}<p class="text-sm" role="status">{m.common_saved()}</p>{/if}
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
      <RecordsPanel item={current} kind={mod.kind} canWrite={canEdit} />
      {#if workspace.role !== 'reader'}
        <div class="flex flex-wrap items-start gap-2">
          <ItemCopy item={current} module={mod} />
          {#if canEdit}
            <Button size="sm" variant="outline" onclick={() => void toggleTemplate()}
              >{current.is_template ? m.modules_template_unset() : m.modules_template_set()}</Button
            >
          {/if}
        </div>
      {/if}
    {/if}

    {#if mod?.kind === 'course'}
      <CoursePanel itemId={current.id} />
    {/if}
    {#if mod?.kind === 'study'}
      <StudyPanel itemId={current.id} />
    {/if}

    {#if mod !== undefined}
      <ItemTimePanel itemId={current.id} moduleKind={mod.kind} canTrack={!current.archived} />
    {/if}

    <AttachmentsPanel target={{ kind: 'item', id: current.id }} canWrite={canEdit} />

    <section class="flex flex-col gap-3" aria-labelledby="item-docs-heading">
      <h2 id="item-docs-heading" class="text-lg font-semibold">{m.modules_documents()}</h2>
      {#if docs.length === 0}
        <p class="text-muted-foreground">{m.modules_documents_empty()}</p>
      {:else}
        <DocumentTree
          {docs}
          label={m.modules_documents_tree({ title: current.title })}
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
              m.modules_new_document_default(),
              parentId,
            );
            router.navigate(itemDocumentPath(workspace.id, current.id, doc.id));
          }}
        />
      {/if}
      {#if canEdit}
        <TemplatePicker
          workspaceId={workspace.id}
          id="new-item-document-template"
          bind:value={docTemplate}
        />
        <CreateForm
          id="new-item-document"
          label={m.modules_new_document_title()}
          placeholder={m.modules_new_document_placeholder()}
          submitLabel={m.modules_create_document()}
          create={async (requestId: string, docTitle: string) => {
            const doc = await api.createItemDocument(
              current.id,
              requestId,
              docTitle,
              null,
              docTemplate === '' ? null : docTemplate,
            );
            router.navigate(itemDocumentPath(workspace.id, current.id, doc.id));
          }}
        />
      {/if}
    </section>
    <TaskBoard
      item={current}
      {workspaces}
      openTaskId={taskId}
      onstale={() => {
        // 권한·보관이 바뀌었을 수 있다. 입력 중인 제목·설명은 두고 항목 상태만 새로 읽는다.
        api
          .item(current.id)
          .then((next) => {
            item = next;
          })
          .catch(() => undefined);
      }}
    />
    <ItemEvents item={current} />
    <CalendarScreen {workspace} scope={{ itemId: current.id }} embedded />
    <Backlinks kind="item" id={current.id} {workspaces} />
  </section>
{/if}
