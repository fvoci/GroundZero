<script lang="ts">
  // 항목 하나: 제목·설명·공개 범위, 보관, 항목 아래 문서 (D-14, D-20, D-59).
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import MarkButtons from '$lib/features/home/MarkButtons.svelte';
  import DeleteButton from '$lib/features/trash/DeleteButton.svelte';
  import type { Module, ModuleKind, Workspace } from '@fvoci/api-client';

  import type { ItemView, SummaryView } from '$lib/api/client';
  import { api, ApiError, newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import AttachmentsPanel from '$lib/features/files/AttachmentsPanel.svelte';
  import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
  import DocumentTree from '$lib/features/documents/DocumentTree.svelte';
  import type { TreeMove } from '$lib/features/documents/tree';
  import { moveThenReload } from '$lib/features/documents/tree';
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
  // 항목 일정 목록에서 바꾸면 달력을, 달력에서 바꾸면 목록을 다시 읽게 하는 값.
  let calendarVersion = $state(0);
  let listVersion = $state(0);
  // 항목 달력의 날짜 칸에서 업무를 만들면 같은 화면의 업무 보드가 다시 읽는다.
  let taskVersion = $state(0);
  let error = $state<string | undefined>(undefined);

  let title = $state('');
  let description = $state('');
  let shared = $state(false);
  let formError = $state<string | undefined>(undefined);
  let saved = $state(false);
  let docTemplate = $state('');

  // 마지막으로 서버 값으로 채운 입력란 값. 입력란이 이 값과 같으면 아직 고치지 않은 것이다.
  let filled = { title: '', description: '', shared: false };

  function fill(next: ItemView): void {
    item = next;
    title = next.title;
    description = next.description;
    shared = next.visibility === 'workspace';
    filled = { title, description, shared };
  }

  /** 항목을 다시 읽었을 때(정보 저장·템플릿 표시 뒤): 고치지 않은 입력란만 새 값으로, 입력 중인 칸은 그대로 둔다. */
  function refresh(next: ItemView): void {
    item = next;
    const nextShared = next.visibility === 'workspace';
    if (title === filled.title) title = next.title;
    if (description === filled.description) description = next.description;
    if (shared === filled.shared) shared = nextShared;
    filled = { title: next.title, description: next.description, shared: nextShared };
  }

  $effect(() => {
    const id = currentItemId;
    let cancelled = false;
    item = undefined;
    mod = undefined;
    error = undefined;
    // 앞 항목의 "저장했습니다."·오류, 새 문서에 고른 템플릿을 새 항목에 남기지 않는다(다른 워크스페이스 항목으로
    // 옮겨 가면 그 템플릿은 목록에 없어 보이지 않는데도 쓰였다).
    saved = false;
    formError = undefined;
    docTemplate = '';
    api
      .item(id)
      .then(async (it) => {
        const [found, list] = await Promise.all([api.module(it.module_id), api.itemDocuments(id)]);
        if (cancelled) return;
        fill(it);
        mod = found;
        workspaceNav.currentItem = { itemId: it.id, moduleId: it.module_id };
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

  // 항목이나 그 모듈이 보관되면 읽기 전용이다(서버의 쓰기 검사와 같다). 항목의 archived는 항목 자신의 보관이므로,
  // 모듈이 보관되면 하위 칸(정보·업무·일정)에는 읽기 접근으로 넘긴다(서버의 문서 접근도 보관되면 읽기로 낮춘다).
  const moduleArchived = $derived(mod?.archived === true);
  const readAccess: ItemView['access'] = 'read';
  const shownItem = $derived(
    item !== undefined && moduleArchived ? { ...item, access: readAccess } : item,
  );
  const canEdit = $derived(
    item !== undefined && !item.archived && !moduleArchived && item.access !== 'read',
  );
  const isOwner = $derived(item?.access === 'owner');

  // 이 화면은 다른 항목으로 옮겨 가도 다시 만들어지지 않는다. 기다리는 사이 옮겨 갔으면 시작한 항목의 결과·오류를
  // 지금 항목 화면에 넣지 않는다(B 주소에 A가 들어앉지 않게).
  const stillOn = (id: string): boolean => currentItemId === id;

  async function save(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (item === undefined) return;
    if (title.trim() === '') {
      formError = m.modules_title_required();
      return;
    }
    formError = undefined;
    saved = false;
    const id = item.id;
    try {
      const next = await api.updateItem(id, {
        title: title.trim(),
        description,
        visibility: shared ? 'workspace' : 'private',
      });
      forgetReferences();
      if (!stillOn(id)) return;
      fill(next);
      saved = true;
    } catch (e: unknown) {
      if (stillOn(id)) formError = errorText(e, m.modules_save_failed());
    }
  }

  async function toggleTemplate(): Promise<void> {
    if (item === undefined) return;
    const id = item.id;
    try {
      await api.setItemTemplate(id, !item.is_template);
      const next = await api.item(id);
      if (stillOn(id)) refresh(next);
    } catch (e: unknown) {
      if (stillOn(id)) formError = errorText(e, m.modules_change_failed());
    }
  }

  async function toggleArchive(): Promise<void> {
    if (item === undefined) return;
    const id = item.id;
    try {
      const next = await api.archiveItem(id, !item.archived);
      if (stillOn(id)) fill(next);
    } catch (e: unknown) {
      if (stillOn(id)) formError = errorText(e, m.modules_change_failed());
    }
  }

  /** 항목을 다시 읽어 입력 중인 칸은 두고 넣는다. 그사이 다른 항목으로 옮겨 갔으면 버린다. */
  function reread(id: string, apply: (next: ItemView) => void): void {
    api
      .item(id)
      .then((next) => {
        if (stillOn(id)) apply(next);
      })
      .catch(() => undefined);
  }

  // 경로: 워크스페이스는 늘(항목을 못 읽어도 돌아갈 곳), 모듈은 읽은 뒤에.
  const crumbs = $derived([
    { path: workspacePath(workspace.id), label: workspace.name },
    ...(mod === undefined
      ? []
      : [{ path: modulePath(workspace.id, mod.id), label: `${mod.name} · ${mod.key}` }]),
  ]);

  // 항목 화면 절로 바로 가기. 업무가 화면 아래쪽에 있어 자주 쓰는 절에 바로 닿게 한다.
  function jumpLinks(kind: ModuleKind | undefined): { id: string; label: string }[] {
    const links = [{ id: 'tasks-heading', label: m.tasks_heading() }];
    if (kind === 'course') links.push({ id: 'course-info-heading', label: m.courses_info() });
    if (kind === 'study') links.push({ id: 'study-heading', label: m.study_heading() });
    if (kind !== undefined) {
      links.push({ id: 'item-profile-heading', label: m.modules_profile() });
      links.push({ id: 'records-heading', label: m.records_heading() });
      links.push({ id: 'item-time-heading', label: m.time_heading() });
    }
    links.push({ id: 'attachments-heading', label: m.files_heading() });
    links.push({ id: 'item-docs-heading', label: m.modules_documents() });
    links.push({ id: 'events-heading', label: m.events_heading() });
    return links;
  }

  function jumpTo(id: string): void {
    const target = document.getElementById(id);
    if (target === null) return;
    target.scrollIntoView({ block: 'start' });
    // 화면 낭독기·키보드도 그 절에서 이어 가도록 제목에 초점을 둔다.
    if (!target.hasAttribute('tabindex')) target.setAttribute('tabindex', '-1');
    target.focus({ preventScroll: true });
  }
</script>

<nav aria-label={m.modules_breadcrumb()} class="-mb-6 min-w-0 text-sm text-muted-foreground">
  <ol class="flex min-w-0 items-center gap-1">
    {#each crumbs as crumb, index (crumb.path)}
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

{#if error !== undefined}
  <p role="alert" class="text-destructive">{error}</p>
{:else if item === undefined}
  <p aria-live="polite">{m.modules_loading()}</p>
{:else}
  {@const current = item}
  {@const view = shownItem ?? current}
  <section class="flex flex-col gap-6" aria-labelledby="item-heading">
    <div class="flex flex-wrap items-baseline gap-2">
      <span class="font-mono text-sm text-muted-foreground">{current.reference}</span>
      <h1 id="item-heading" class="text-xl font-semibold">{current.title}</h1>
      <Badge variant="outline"
        >{current.visibility === 'private'
          ? m.modules_private()
          : m.modules_shared_workspace()}</Badge
      >
      {#if current.archived || moduleArchived}<Badge variant="secondary"
          >{m.modules_archived_readonly()}</Badge
        >{/if}
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

    <!-- 긴 항목 화면 안에서 바로 가기. 모듈 유형에 있는 절만 보인다. -->
    <nav aria-label={m.modules_jump_label()} class="-mt-2 flex flex-wrap gap-1 text-sm">
      {#each jumpLinks(mod?.kind) as link (link.id)}
        <a
          class="rounded-md border px-2 py-0.5 text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          href={`#${link.id}`}
          onclick={(e: MouseEvent) => {
            e.preventDefault();
            jumpTo(link.id);
          }}>{link.label}</a
        >
      {/each}
    </nav>

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
        item={view}
        kind={mod.kind}
        onchanged={() => {
          reread(current.id, refresh);
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
      <DocumentTree
        {docs}
        label={m.modules_documents_tree({ title: current.title })}
        empty={m.modules_documents_empty()}
        hrefFor={(id: string) => itemDocumentPath(workspace.id, current.id, id)}
        navigate={(path: string) => {
          router.navigate(path);
        }}
        canWrite={canEdit}
        onmove={(id: string, move: TreeMove) =>
          // 실패해도(권한 철회·동시 변경) 최신 목록을 다시 읽는다.
          moveThenReload(
            () => api.setParent(id, move.parentId, move.beforeId),
            async () => {
              // 다시 읽는 사이 다른 항목으로 옮겼으면 이 항목의 목록을 그 화면에 넣지 않는다.
              const requested = current.id;
              const list = await api.itemDocuments(requested);
              if (currentItemId === requested) docs = list;
            },
          )}
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
      item={view}
      {workspaces}
      openTaskId={taskId}
      tasksVersion={taskVersion}
      onstale={() => {
        // 권한·보관이 바뀌었을 수 있다. 입력 중인 제목·설명은 두고 항목 상태만 새로 읽는다.
        reread(current.id, (next: ItemView) => {
          item = next;
        });
      }}
    />
    <!-- 일정 목록과 달력은 같은 일정을 따로 읽는다. 한쪽에서 바꾸면 다른 쪽이 다시 읽게 알린다. -->
    <ItemEvents
      item={view}
      version={listVersion}
      onchanged={() => {
        calendarVersion += 1;
      }}
    />
    <CalendarScreen
      {workspace}
      scope={{ itemId: current.id }}
      item={view}
      embedded
      version={calendarVersion}
      onchanged={() => {
        listVersion += 1;
      }}
      ontaskcreated={() => {
        taskVersion += 1;
      }}
    />
    <Backlinks kind="item" id={current.id} {workspaces} />
  </section>
{/if}
