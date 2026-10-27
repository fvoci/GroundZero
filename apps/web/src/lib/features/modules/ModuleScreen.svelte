<script lang="ts">
  // 모듈 하나: 설명, 항목 목록과 항목 만들기, 관리자의 이름·키·아이콘·설명 변경과 보관·휴지통 (D-15, D-16, D-18, D-78).
  import type { Module, Workspace } from '@fvoci/api-client';
  import { onMount } from 'svelte';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import type { CreateFormContext } from '$lib/features/common/form-draft';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { object, strings, retry } from './draft-values';

  import type { ItemView } from '$lib/api/client';
  import { api, ApiError, newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { forgetReferences } from '$lib/features/references/resolver';
  import CalendarScreen from '$lib/features/events/CalendarScreen.svelte';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { workspaceNav } from '$lib/features/nav/workspace-nav.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { me } from '$lib/me.svelte';
  import DeleteButton from '$lib/features/trash/DeleteButton.svelte';
  import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
  import { itemPath, router, workspacePath } from '$lib/router.svelte';
  import { Badge } from '$lib/ui/badge';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import { NativeSelect } from '$lib/ui/native-select';
  import { Textarea } from '$lib/ui/textarea';

  import { kindLabel, normalizeKey } from './kinds';
  import { itemStatusLabel, periodText } from './profile';

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
    // 템플릿에서 만들기는 모듈마다 따로다. 앞 모듈에서 고른 템플릿·제목·오류를 새 모듈에 남기지 않는다.
    templateId = '';
    templateTitle = '';
    templateError = undefined;
    Promise.all([api.module(id), api.items(id)])
      .then(([found, list]) => {
        if (cancelled) return;
        mod = found;
        items = list;
        editName = found.name;
        editKey = found.key;
        editIcon = found.icon;
        editDescription = found.description;
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        error =
          e instanceof ApiError && e.status === 404
            ? m.modules_not_found()
            : m.modules_load_failed();
      });
    return () => {
      cancelled = true;
    };
  });

  // 관리·쓰기 여부는 서버의 접근 등급(app_access)에서 온다(D-113). 보관은 등급과 별개로 쓰기를 막는다.
  const isAdmin = $derived(workspace.access === 'owner');
  const canCreate = $derived(workspace.access !== 'read' && mod?.archived === false);

  let editName = $state('');
  let editKey = $state('');
  let editIcon = $state('');
  let editDescription = $state('');
  let settingsError = $state<string | undefined>(undefined);
  let settingsNote = $state<string | undefined>(undefined);
  /**
   * 설정을 저장하는 중. 저장은 요청 여러 개라 두 번 겹치면 먼저 시작해 늦게 실패한 저장이 이미 저장된 일부(아이콘·설명)만
   * 든 옛 모듈을 화면에 넣어, 뒤 저장이 넣은 이름·키를 되돌려 보였다. 끝날 때까지 다시 보내지 않는다.
   */
  let savingSettings = $state(false);
  const settingsDraft = createFormDraft({
    name: () => `module-settings:${moduleId}`,
    snapshot: () => ({
      name: editName,
      key: editKey,
      icon: editIcon,
      description: editDescription,
    }),
    parse: (raw) => {
      if (
        !strings(raw) ||
        raw.name === undefined ||
        raw.key === undefined ||
        raw.icon === undefined ||
        raw.description === undefined
      )
        return undefined;
      return { name: raw.name, key: raw.key, icon: raw.icon, description: raw.description };
    },
    restore: (value) => {
      editName = value.name;
      editKey = value.key;
      editIcon = value.icon;
      editDescription = value.description;
    },
    reset: () => {
      editName = mod?.name ?? '';
      editKey = mod?.key ?? '';
      editIcon = mod?.icon ?? '';
      editDescription = mod?.description ?? '';
      savingSettings = false;
    },
    dirty: () =>
      mod !== undefined &&
      (editName !== mod.name ||
        editKey !== mod.key ||
        editIcon !== mod.icon ||
        editDescription !== mod.description),
    busy: () => savingSettings,
  });
  onMount(settingsDraft.attach);

  async function saveSettings(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (mod === undefined || savingSettings) return;
    const attempt = settingsDraft.capture();
    if (attempt === undefined) return;
    const key = normalizeKey(editKey);
    if (editName.trim() === '' || key === undefined) {
      settingsError = m.modules_settings_invalid();
      return;
    }
    settingsError = undefined;
    settingsNote = undefined;
    // 저장을 시작한 모듈과 입력값을 붙잡아 둔다. 기다리는 사이 다른 모듈로 옮겨 가도 그 모듈에 보내거나 그 화면에
    // 결과를 넣지 않는다(이 화면은 모듈이 바뀌어도 다시 만들어지지 않는다).
    const target = mod;
    const edits = {
      name: editName.trim(),
      icon: editIcon.trim(),
      description: editDescription.trim(),
    };
    const stillHere = (): boolean => moduleId === target.id && settingsDraft.isOwner(attempt);
    // 아이콘·설명·이름과 키는 따로 저장된다. 뒤 요청이 막혀도(키 409 등) 이미 저장된 앞 부분은 화면에 넣는다.
    let partial: Module | undefined;
    savingSettings = true;
    try {
      const before = target.key;
      if (edits.icon !== target.icon) {
        await api.setModuleIcon(target.id, edits.icon);
        if (me.userId !== attempt.userId) return;
        partial = { ...target, icon: edits.icon };
      }
      if (edits.description !== target.description) {
        partial = await api.setModuleDescription(target.id, edits.description);
        if (me.userId !== attempt.userId) return;
      }
      const saved = await api.updateModule(target.id, edits.name, key);
      partial = undefined;
      workspaceNav.refreshModules();
      forgetReferences();
      if (!stillHere()) return;
      mod = saved;
      const completed = settingsDraft.complete(attempt);
      const list = await api.items(saved.id);
      if (!stillHere()) return;
      items = list;
      if (completed) notifyDone(m.common_saved());
      if (before !== saved.key) {
        settingsNote = m.modules_key_changed({ before, after: saved.key });
      }
    } catch (e: unknown) {
      if (partial !== undefined) {
        workspaceNav.refreshModules();
        if (stillHere()) mod = partial;
      }
      if (!stillHere()) return;
      settingsError =
        e instanceof ApiError && e.status === 409
          ? m.modules_key_taken()
          : errorText(e, m.modules_change_failed());
    } finally {
      if (settingsDraft.isOwner(attempt)) savingSettings = false;
    }
  }

  // 템플릿에서 만들기(D-33): 새로 시작(상태·날짜·체크 비우기)으로 복제한다.
  const templates = $derived(items.filter((i) => i.is_template && !i.archived));
  let templateId = $state('');
  let templateTitle = $state('');
  let templateError = $state<string | undefined>(undefined);
  let templateBusy = $state(false);
  // 같은 템플릿·제목의 재시도는 같은 요청 id, 고치면 새 id.
  let templatePending = $state<{ requestId: string; signature: string } | null>(null);
  const templateDraft = createFormDraft({
    name: () => `module-template:${moduleId}`,
    snapshot: () => ({ title: templateTitle, source: templateId, pending: templatePending }),
    parse: (raw) => {
      if (
        !object(raw) ||
        typeof raw.title !== 'string' ||
        typeof raw.source !== 'string' ||
        !retry(raw.pending)
      )
        return undefined;
      return { title: raw.title, source: raw.source, pending: raw.pending };
    },
    restore: (value) => {
      templateTitle = value.title;
      templateId = value.source;
      templatePending = value.pending;
    },
    reset: () => {
      templateTitle = '';
      templateId = '';
      templatePending = null;
      templateError = undefined;
      templateBusy = false;
    },
    dirty: () => templateTitle !== '' || templateId !== '',
    busy: () => templateBusy,
  });
  onMount(templateDraft.attach);

  async function fromTemplate(moduleId: string): Promise<void> {
    if (templateBusy || !templateDraft.active || templateDraft.blocked) return;
    // 지금 모듈의 템플릿 목록에 있는 것만 보낸다. 고르지 않았으면 첫 템플릿. 고른 템플릿이 다시 읽은 목록에서
    // 빠졌으면(보관·템플릿 해제) 선택 칸은 빈 칸으로 보이지만 값은 남는다. 고르지 않은 첫 템플릿으로 바꾸지 않고
    // 다시 고르게 한다.
    const source =
      templateId === '' ? templates[0]?.id : templates.find((t) => t.id === templateId)?.id;
    if (source === undefined || templateTitle.trim() === '') {
      templateError = m.modules_template_required();
      return;
    }
    templateError = undefined;
    let attempt: ReturnType<typeof templateDraft.capture>;
    try {
      const title = templateTitle.trim();
      const signature = JSON.stringify({ source, moduleId, title });
      if (templatePending?.signature !== signature)
        templatePending = { requestId: newRequestId(), signature };
      const requestId = templatePending.requestId;
      attempt = templateDraft.capture();
      if (attempt === undefined) return;
      templateBusy = true;
      const copied = await api.copyItem(source, requestId, moduleId, title, true);
      if (!templateDraft.complete(attempt)) return;
      router.navigate(itemPath(workspace.id, copied.item_id));
    } catch (e: unknown) {
      if (attempt === undefined || !templateDraft.isCurrent(attempt)) return;
      templateError = errorText(e, m.modules_create_failed());
    } finally {
      if (attempt !== undefined && templateDraft.isOwner(attempt)) templateBusy = false;
    }
  }

  async function toggleArchive(): Promise<void> {
    if (mod === undefined) return;
    try {
      mod = await api.archiveModule(mod.id, !mod.archived);
      workspaceNav.refreshModules();
    } catch (e: unknown) {
      settingsError = errorText(e, m.modules_change_failed());
    }
  }
</script>

{#each mod === undefined ? [] : settingsDraft.offers as entry (entry.key)}
  <FormDraftOffer
    onrestore={() => {
      settingsDraft.restore(entry);
    }}
    ondiscard={() => {
      settingsDraft.discard(entry);
    }}
  />
{/each}
<DiscardChangesConfirm
  open={settingsDraft.leave.open}
  onkeep={settingsDraft.leave.keep}
  ondiscard={settingsDraft.discardAndLeave}
/>
<!-- 템플릿에서 만드는 동안 입력과 되살리기를 잠근다(D-97). 그사이 바뀐 입력은 성공한 만들기를 버리게 하고, 다시 내면
     새 요청 id로 두 번째 항목을 만든다. -->
{#each mod === undefined ? [] : templateDraft.offers as entry (entry.key)}
  <fieldset class="contents" disabled={templateBusy}>
    <FormDraftOffer
      onrestore={() => {
        templateDraft.restore(entry);
      }}
      ondiscard={() => {
        templateDraft.discard(entry);
      }}
    />
  </fieldset>
{/each}
<DiscardChangesConfirm
  open={templateDraft.leave.open}
  onkeep={templateDraft.leave.keep}
  ondiscard={templateDraft.discardAndLeave}
/>

{#if error !== undefined}
  <p role="alert" class="text-destructive">{error}</p>
{:else if mod === undefined}
  <p aria-live="polite">{m.modules_loading()}</p>
{:else}
  {@const current = mod}
  <section class="flex flex-col gap-6" aria-labelledby="module-heading">
    <div class="flex flex-wrap items-baseline gap-2">
      <h1 id="module-heading" class="text-xl font-semibold">
        {#if current.icon !== ''}<span aria-hidden="true">{current.icon}</span>{/if}
        {current.name}
      </h1>
      <span class="font-mono text-sm text-muted-foreground">{current.key}</span>
      <Badge variant="outline">{kindLabel(current.kind)}</Badge>
      {#if current.archived}<Badge variant="secondary">{m.modules_archived_readonly()}</Badge>{/if}
      {#if isAdmin}
        <div class="ml-auto">
          <DeleteButton
            what={m.modules_delete_module_what()}
            remove={() => api.trashModule(current.id)}
            ondeleted={() => {
              workspaceNav.refreshModules();
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
      <p class="text-sm text-muted-foreground">
        {m.modules_former_keys({ keys: current.previous_keys.join(', ') })}
      </p>
    {/if}

    {#if items.length === 0}
      <p class="rounded-md border bg-card p-2 @md:p-3 text-muted-foreground">
        {m.modules_items_empty()}
      </p>
    {:else}
      <ul class="flex flex-col gap-1 rounded-md border bg-card p-2" data-testid="item-list">
        {#each items as it (it.id)}
          <li>
            <a
              class="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 rounded px-2 py-1.5 hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
              href={itemPath(workspace.id, it.id)}
              onclick={(e) => {
                e.preventDefault();
                router.navigate(itemPath(workspace.id, it.id));
              }}
            >
              <span class="shrink-0 font-mono text-sm whitespace-nowrap text-muted-foreground"
                >{it.reference}</span
              >
              <span class="min-w-0 [overflow-wrap:anywhere]">{it.title}</span>
              <Badge variant="outline">{itemStatusLabel(it.status)}</Badge>
              {#if it.starts_on !== null || it.ends_on !== null}
                <span class="text-sm text-muted-foreground"
                  >{periodText(it.starts_on, it.ends_on)}</span
                >
              {/if}
              {#if it.visibility === 'private'}<Badge variant="outline">{m.modules_private()}</Badge
                >{/if}
              {#if it.archived}<Badge variant="secondary">{m.modules_archived()}</Badge>{/if}
              {#if it.is_template}<Badge variant="secondary">{m.modules_template()}</Badge>{/if}
            </a>
          </li>
        {/each}
      </ul>
    {/if}

    <!-- 기능 단위의 큰 구획(항목 목록·만들기·달력·설정)만 얇은 테두리로 나눈다. 행·입력 칸에는 박스를 두르지 않는다. -->
    {#if canCreate}
      <div class="flex flex-col gap-2 rounded-md border bg-card p-2 @md:p-3">
        <CreateForm
          id="new-item"
          draftName={`module-item-new:${current.id}`}
          label={m.modules_new_item_title()}
          placeholder={m.modules_new_item_placeholder()}
          submitLabel={m.modules_create_item()}
          create={async (requestId: string, title: string, context: CreateFormContext) => {
            const created = await api.createItem(current.id, requestId, title);
            if (context.saved()) router.navigate(itemPath(workspace.id, created.id));
          }}
        />
        <p class="text-sm text-muted-foreground">{m.modules_new_item_hint()}</p>
        {#if templates.length > 0 || templateTitle !== '' || templateId !== ''}
          <fieldset class="min-w-0" disabled={templateBusy}>
            <form
              class="flex flex-wrap items-end gap-2"
              aria-label={m.modules_from_template()}
              onsubmit={(e: SubmitEvent) => {
                e.preventDefault();
                void fromTemplate(current.id);
              }}
            >
              <div class="flex flex-col gap-1">
                <Label for="template-source">{m.modules_template()}</Label>
                <NativeSelect id="template-source" class="w-full" bind:value={templateId}>
                  {#each templates as t (t.id)}<option value={t.id}>{t.title}</option>{/each}
                </NativeSelect>
              </div>
              <div class="flex min-w-0 flex-1 basis-40 flex-col gap-1">
                <Label for="template-title">{m.modules_template_item_title()}</Label>
                <Input id="template-title" bind:value={templateTitle} maxlength={200} />
              </div>
              <Button type="submit" variant="outline">{m.modules_from_template()}</Button>
            </form>
          </fieldset>
          {#if templateError !== undefined}
            <p class="text-sm text-destructive" role="alert">{templateError}</p>
          {/if}
        {/if}
      </div>
    {/if}

    <div class="rounded-md border bg-card p-2 @md:p-3">
      <CalendarScreen
        {workspace}
        scope={{ moduleId: current.id }}
        locked={current.archived}
        embedded
      />
    </div>

    {#if isAdmin}
      <form
        class="flex flex-wrap items-end gap-2 rounded-md border bg-card p-2 @md:p-3"
        onsubmit={(e) => void saveSettings(e)}
        novalidate
        aria-label={m.modules_settings()}
      >
        <div class="flex min-w-0 flex-1 basis-40 flex-col gap-1">
          <Label for="module-edit-name">{m.modules_name()}</Label>
          <Input id="module-edit-name" bind:value={editName} maxlength={200} />
        </div>
        <div class="flex w-28 flex-col gap-1">
          <Label for="module-edit-key">{m.modules_key()}</Label>
          <Input
            id="module-edit-key"
            bind:value={editKey}
            maxlength={10}
            autocomplete="off"
            class="font-mono uppercase"
          />
        </div>
        <div class="flex w-24 flex-col gap-1">
          <Label for="module-edit-icon">{m.modules_icon()}</Label>
          <Input
            id="module-edit-icon"
            bind:value={editIcon}
            maxlength={16}
            placeholder={m.modules_icon_placeholder()}
          />
        </div>
        <div class="flex w-full flex-col gap-1">
          <Label for="module-edit-description">{m.modules_description()}</Label>
          <Textarea
            id="module-edit-description"
            bind:value={editDescription}
            maxlength={2000}
            rows={3}
            class="min-h-20"
          />
        </div>
        <Button type="submit" variant="outline" disabled={savingSettings}>{m.common_save()}</Button>
        <Button type="button" variant="outline" onclick={() => void toggleArchive()}
          >{current.archived ? m.modules_unarchive() : m.modules_archive()}</Button
        >
      </form>
      <p class="text-sm empty:hidden" role="status">
        {#if settingsNote !== undefined}{settingsNote}{/if}
      </p>
      {#if settingsError !== undefined}
        <p class="text-sm text-destructive" role="alert">{settingsError}</p>
      {/if}
    {/if}
  </section>
{/if}
