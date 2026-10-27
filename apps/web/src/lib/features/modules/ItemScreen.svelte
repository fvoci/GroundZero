<script lang="ts">
  // 항목 하나: 제목·설명·공개 범위, 보관, 항목 아래 문서 (D-14, D-20, D-59).
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import MarkButtons from '$lib/features/home/MarkButtons.svelte';
  import DeleteButton from '$lib/features/trash/DeleteButton.svelte';
  import type { Module, ModuleKind, Workspace } from '@fvoci/api-client';
  import { onMount } from 'svelte';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import type { CreateFormContext } from '$lib/features/common/form-draft';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { object } from './draft-values';

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
  import { Textarea } from '$lib/ui/textarea';

  import ItemCopy from './ItemCopy.svelte';
  import ItemProfilePanel from './ItemProfilePanel.svelte';
  // 활동(D-119). 업무 보기 탭과 별개인 항목 활동 섹션.
  import ItemActivityTab from '$lib/features/activity/ItemActivityTab.svelte';

  interface Props {
    workspace: Workspace;
    itemId: string;
    /** 주소에서 연 업무(상세 대화상자). */
    taskId?: string | undefined;
    workspaces: Workspace[];
  }

  const { workspace, itemId, workspaces, taskId }: Props = $props();
  // 같은 항목 안 보기·상세 경로 변경도 업무 화면에 즉시 전달한다(D-121).
  const taskRoute = $derived(
    router.route.name === 'item' && router.route.itemId === itemId ? router.route : undefined,
  );
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

  // 마지막으로 서버 값으로 채운 입력란 값. 입력란이 이 값과 같으면 아직 고치지 않은 것이다. `version`은 저장할 때 보낼
  // 기준 판(D-104)이다: 입력 중에 다시 읽은 항목의 판으로 옮기면 그사이 다른 사람이 고친 것을 덮으므로, 입력란을
  // 서버 값으로 채울 때(또는 내 저장의 응답)만 옮긴다.
  let filled = $state({ title: '', description: '', shared: false, version: 0 });
  const infoDraft = createFormDraft({
    name: () => `item-info:${itemId}`,
    snapshot: () => ({ title, description, shared, filled }),
    parse: (raw) => {
      if (
        !object(raw) ||
        typeof raw.title !== 'string' ||
        typeof raw.description !== 'string' ||
        typeof raw.shared !== 'boolean' ||
        !object(raw.filled)
      )
        return undefined;
      const base = raw.filled;
      if (
        typeof base.title !== 'string' ||
        typeof base.description !== 'string' ||
        typeof base.shared !== 'boolean' ||
        typeof base.version !== 'number'
      )
        return undefined;
      return {
        title: raw.title,
        description: raw.description,
        shared: raw.shared,
        filled: {
          title: base.title,
          description: base.description,
          shared: base.shared,
          version: base.version,
        },
      };
    },
    restore: (value) => {
      title = value.title;
      description = value.description;
      shared = value.shared;
      filled = value.filled;
    },
    reset: () => {
      title = item?.title ?? '';
      description = item?.description ?? '';
      shared = item?.visibility === 'workspace';
      filled = { title, description, shared, version: item?.version ?? 0 };
      saving = false;
    },
    dirty: () =>
      item !== undefined &&
      (title !== filled.title || description !== filled.description || shared !== filled.shared),
    busy: () => saving,
  });
  onMount(infoDraft.attach);
  // 저장이 판 충돌(409)로 거절됐을 때의 서버 최신본. 입력은 그대로 두고 최신본을 보인다.
  let conflict = $state<ItemView | undefined>(undefined);

  function fill(next: ItemView): void {
    item = next;
    title = next.title;
    description = next.description;
    shared = next.visibility === 'workspace';
    filled = { title, description, shared, version: next.version };
    conflict = undefined;
  }

  /**
   * 항목을 다시 읽었을 때(정보 저장·템플릿 표시 뒤): 고치지 않은 입력란만 새 값으로, 입력 중인 칸은 그대로 둔다.
   * `base`는 "고치지 않았다"를 가를 기준 값(기본은 마지막으로 채운 서버 값, 저장 응답에서는 보낸 값). `own`이면 내 저장의
   * 응답이라 그 판을 다음 저장의 기준으로 삼는다. 아니면 입력 중인 칸이 없을 때만 옮긴다(입력 중이면 옛 판을 두어, 그사이
   * 바뀐 것을 저장이 409로 알린다).
   */
  function refresh(
    next: ItemView,
    base: { title: string; description: string; shared: boolean } = filled,
    own = false,
  ): void {
    item = next;
    const nextShared = next.visibility === 'workspace';
    const editing =
      title !== base.title || description !== base.description || shared !== base.shared;
    if (title === base.title) title = next.title;
    if (description === base.description) description = next.description;
    if (shared === base.shared) shared = nextShared;
    filled = {
      title: next.title,
      description: next.description,
      shared: nextShared,
      version: own || !editing ? next.version : filled.version,
    };
  }

  // 항목을 (다시) 불러올 때마다 늘리는 차례. A → B → A로 돌아오면 항목 id는 같아도 새로 읽은 화면이다.
  let loadSeq = 0;

  $effect(() => {
    const id = currentItemId;
    loadSeq += 1;
    let cancelled = false;
    item = undefined;
    mod = undefined;
    error = undefined;
    // 앞 항목의 "저장했습니다."·오류, 새 문서에 고른 템플릿을 새 항목에 남기지 않는다(다른 워크스페이스 항목으로
    // 옮겨 가면 그 템플릿은 목록에 없어 보이지 않는데도 쓰였다).
    saved = false;
    formError = undefined;
    conflict = undefined;
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

  // 접근 등급은 서버 app_access 결과를 그대로 쓴다(D-113). 보관은 등급이 아니라 아래 canEdit에서 본다.
  const moduleArchived = $derived(mod?.archived === true);
  const shownItem = $derived(item);
  // 보관된 항목과 보관된 모듈의 항목은 읽기 전용이다(서버도 쓰기를 거부한다: app_item_archived 55000). 등급(app_grade_item)은 보관을
  // 반영하지 않으므로 소유자·쓰기 권한자에게도 편집 폼과 "보관" 단추 대신 아래의 "보관 해제" 단추만 보인다.
  const canEdit = $derived(
    item !== undefined && item.access !== 'read' && !item.archived && !moduleArchived,
  );
  const isOwner = $derived(item?.access === 'owner');

  // 이 화면은 다른 항목으로 옮겨 가도 다시 만들어지지 않는다. 기다리는 사이 옮겨 갔으면 시작한 항목의 결과·오류를
  // 지금 항목 화면에 넣지 않는다(B 주소에 A가 들어앉지 않게). 같은 항목으로 돌아왔어도 시작할 때의 차례(`mine`)가
  // 아니면 버린다(앞 A 화면의 늦은 저장이 새 A 화면에 "저장했습니다."를 띄우거나 새로 읽은 값을 옛 값으로 되돌렸다).
  const stillOn = (id: string, mine: number): boolean => currentItemId === id && loadSeq === mine;

  /**
   * 항목 정보를 저장하는 중. 공개 범위는 체크를 바꿨을 때만 보내므로, 공개로 바꾼 저장이 끝나기 전에 체크를 풀고 다시
   * 저장하면 그 저장은 아직 비공개인 `filled`와 같아 공개 범위를 보내지 않았다. 먼저 보낸 공개가 서버에 남는데 화면은
   * 비공개로 보였다. 끝날 때까지 다시 보내지 않고, 다음 저장은 응답으로 채운 값과 비교한다.
   */
  let saving = $state(false);

  async function save(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    await submit(filled.version);
  }

  /**
   * 입력란을 `baseVersion` 판에 대한 고치기로 보낸다. 보통은 입력란을 채운 판이고, 충돌 알림의 "내 입력으로 다시
   * 저장"은 알림이 보인 최신본의 판이다(그 뒤 또 바뀌었으면 다시 409).
   */
  async function submit(baseVersion: number): Promise<void> {
    if (item === undefined || saving) return;
    const attempt = infoDraft.capture();
    if (attempt === undefined) return;
    if (title.trim() === '') {
      formError = m.modules_title_required();
      return;
    }
    formError = undefined;
    saved = false;
    const id = item.id;
    const mine = loadSeq;
    // 공개 범위는 체크를 바꿨을 때만 보낸다. 늘 보내면 연 뒤 다른 소유 권한자가 바꾼 공개 범위를 되돌렸다(감사 W13).
    const visibility: 'workspace' | 'private' | undefined =
      shared === filled.shared ? undefined : shared ? 'workspace' : 'private';
    // 보낸 입력. 응답을 기다리는 사이 더 고친 칸은 응답 값으로 덮지 않는다(보내지 않은 입력이 사라지지 않게).
    const sent = { title, description, shared };
    saving = true;
    try {
      const result = await api.updateItem(id, baseVersion, {
        title: title.trim(),
        description,
        ...(visibility === undefined ? {} : { visibility }),
      });
      if (result.kind === 'conflict') {
        // 그사이 다른 사람이 고쳤다. 서버는 아무것도 바꾸지 않았다. 입력은 그대로 두고 최신본을 보인다(D-104).
        if (stillOn(id, mine) && infoDraft.isOwner(attempt)) conflict = result.current;
        return;
      }
      forgetReferences();
      if (!stillOn(id, mine) || !infoDraft.isOwner(attempt)) return;
      if (infoDraft.isCurrent(attempt)) {
        item = result.item;
        infoDraft.complete(attempt);
      }
      conflict = undefined;
      refresh(result.item, sent, true);
      // 더 고친 칸이 남아 있으면 "저장했습니다."를 보이지 않는다(그 입력은 아직 저장되지 않았다).
      saved =
        title === filled.title && description === filled.description && shared === filled.shared;
    } catch (e: unknown) {
      if (stillOn(id, mine) && infoDraft.isOwner(attempt))
        formError = errorText(e, m.modules_save_failed());
    } finally {
      if (infoDraft.isOwner(attempt)) saving = false;
    }
  }

  async function toggleTemplate(): Promise<void> {
    if (item === undefined) return;
    const id = item.id;
    const mine = loadSeq;
    try {
      await api.setItemTemplate(id, !item.is_template);
      const next = await api.item(id);
      if (stillOn(id, mine)) refresh(next);
    } catch (e: unknown) {
      if (stillOn(id, mine)) formError = errorText(e, m.modules_change_failed());
    }
  }

  async function toggleArchive(): Promise<void> {
    if (item === undefined) return;
    const id = item.id;
    const mine = loadSeq;
    try {
      const next = await api.archiveItem(id, !item.archived);
      if (stillOn(id, mine)) fill(next);
    } catch (e: unknown) {
      if (stillOn(id, mine)) formError = errorText(e, m.modules_change_failed());
    }
  }

  /** 항목을 다시 읽어 입력 중인 칸은 두고 넣는다. 그사이 다른 항목으로 옮겨 갔으면(돌아왔어도) 버린다. */
  function reread(id: string, apply: (next: ItemView) => void): void {
    const mine = loadSeq;
    api
      .item(id)
      .then((next) => {
        if (stillOn(id, mine)) apply(next);
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

{#each item === undefined ? [] : infoDraft.offers as entry (entry.key)}
  <FormDraftOffer
    onrestore={() => {
      infoDraft.restore(entry);
    }}
    ondiscard={() => {
      infoDraft.discard(entry);
    }}
  />
{/each}
<DiscardChangesConfirm
  open={infoDraft.leave.open}
  onkeep={infoDraft.leave.keep}
  ondiscard={infoDraft.discardAndLeave}
/>

<nav aria-label={m.modules_breadcrumb()} class="-mb-6 min-w-0 text-sm text-muted-foreground">
  <ol class="flex min-w-0 items-center gap-1">
    {#each crumbs as crumb, index (crumb.path)}
      <li class="flex max-w-full min-w-0 shrink items-center gap-1">
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
          <Textarea
            id="item-description"
            bind:value={description}
            maxlength={2000}
            rows={3}
            class="min-h-20"
          />
        </div>
        {#if isOwner}
          <div class="flex items-center gap-2">
            <Checkbox id="item-shared" bind:checked={shared} />
            <Label for="item-shared">{m.modules_share_with_members()}</Label>
          </div>
        {/if}
        <div class="flex gap-2">
          <Button type="submit" disabled={saving}>{m.common_save()}</Button>
          {#if isOwner}
            <Button type="button" variant="outline" onclick={() => void toggleArchive()}
              >{m.modules_archive()}</Button
            >
          {/if}
        </div>
      </form>
      {#if conflict !== undefined}
        {@const latest = conflict}
        <!-- 판 충돌(D-104): 입력은 그대로 두고 서버 최신본을 보인다. 최신본으로 바꾸거나 내 입력으로 다시 저장한다. -->
        <div class="rounded-md border border-amber-500/50 p-3 text-sm" data-item-conflict>
          <p role="alert">{m.modules_item_conflict()}</p>
          <dl class="mt-2 grid gap-1">
            <dt class="text-muted-foreground">{m.modules_title()}</dt>
            <dd class="break-words whitespace-pre-wrap" data-latest="title">{latest.title}</dd>
            <dt class="text-muted-foreground">{m.modules_description()}</dt>
            <dd class="break-words whitespace-pre-wrap" data-latest="description">
              {latest.description === '' ? m.modules_item_conflict_empty() : latest.description}
            </dd>
            {#if isOwner}
              <dt class="text-muted-foreground">{m.modules_share_with_members()}</dt>
              <dd data-latest="visibility">
                {latest.visibility === 'workspace'
                  ? m.modules_item_conflict_shared()
                  : m.modules_item_conflict_not_shared()}
              </dd>
            {/if}
          </dl>
          <div class="mt-2 flex flex-wrap gap-2">
            <Button
              size="sm"
              variant="outline"
              onclick={() => {
                fill(latest);
                document.getElementById('item-title')?.focus();
              }}>{m.modules_item_conflict_load()}</Button
            >
            <Button size="sm" disabled={saving} onclick={() => void submit(latest.version)}
              >{m.modules_item_conflict_save_mine()}</Button
            >
          </div>
        </div>
      {/if}
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
    <p class="text-sm empty:hidden" role="status">
      {#if saved}{m.common_saved()}{/if}
    </p>
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
      {#if workspace.access !== 'read'}
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
      <!-- 항목은 내 합계만 보며 시작 설정은 없다(D-117). -->
      <ItemTimePanel itemId={current.id} />
    {/if}

    <AttachmentsPanel target={{ kind: 'item', id: current.id }} canWrite={canEdit} />
    <!-- 활동(D-119) -->
    <ItemActivityTab itemId={current.id} workspaceId={workspace.id} />

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
              // 다시 읽는 사이 다른 항목으로 옮겼으면(같은 항목으로 돌아왔어도) 이 목록을 그 화면에 넣지 않는다.
              const requested = current.id;
              const mine = loadSeq;
              const list = await api.itemDocuments(requested);
              if (stillOn(requested, mine)) docs = list;
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
          draftName={`item-document-new:${current.id}`}
          bind:draftContext={docTemplate}
          label={m.modules_new_document_title()}
          placeholder={m.modules_new_document_placeholder()}
          submitLabel={m.modules_create_document()}
          create={async (requestId: string, docTitle: string, context: CreateFormContext) => {
            const doc = await api.createItemDocument(
              current.id,
              requestId,
              docTitle,
              null,
              docTemplate === '' ? null : docTemplate,
            );
            if (context.saved())
              router.navigate(itemDocumentPath(workspace.id, current.id, doc.id));
          }}
        />
      {/if}
    </section>
    <TaskBoard
      item={view}
      {workspaces}
      routeView={taskRoute?.view}
      openTaskId={taskRoute === undefined ? taskId : taskRoute.taskId}
      tasksVersion={taskVersion}
      locked={moduleArchived}
      onstale={() => {
        // 권한·보관이 바뀌었을 수 있다. 입력 중인 칸은 두고, 항목 상태와 고치지 않은 칸은 새로 읽은 값으로 바꾼다
        // (옛 값이 남으면 다음 저장이 그사이 바뀐 제목·설명을 되돌린다).
        reread(current.id, refresh);
      }}
    />
    <!-- 일정 목록과 달력은 같은 일정을 따로 읽는다. 한쪽에서 바꾸면 다른 쪽이 다시 읽게 알린다. -->
    <ItemEvents
      item={view}
      locked={moduleArchived}
      version={listVersion}
      onchanged={() => {
        calendarVersion += 1;
      }}
    />
    <CalendarScreen
      {workspace}
      scope={{ itemId: current.id }}
      item={view}
      locked={moduleArchived || view.archived}
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
