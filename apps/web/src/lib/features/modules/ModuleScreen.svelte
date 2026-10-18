<script lang="ts">
  // 모듈 하나: 설명, 항목 목록과 항목 만들기, 관리자의 이름·키·아이콘·설명 변경과 보관·휴지통 (D-15, D-16, D-18, D-78).
  import type { Module, Workspace } from '@fvoci/api-client';

  import type { ItemView } from '$lib/api/client';
  import { api, ApiError, newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { forgetReferences } from '$lib/features/references/resolver';
  import CalendarScreen from '$lib/features/events/CalendarScreen.svelte';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { workspaceNav } from '$lib/features/nav/workspace-nav.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import DeleteButton from '$lib/features/trash/DeleteButton.svelte';
  import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
  import { itemPath, router, workspacePath } from '$lib/router.svelte';
  import { Badge } from '$lib/ui/badge';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

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
      settingsError = m.modules_settings_invalid();
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
      workspaceNav.refreshModules();
      forgetReferences();
      editKey = mod.key;
      items = await api.items(mod.id);
      notifyDone(m.common_saved());
      if (before !== mod.key) {
        settingsNote = m.modules_key_changed({ before, after: mod.key });
      }
    } catch (e: unknown) {
      settingsError =
        e instanceof ApiError && e.status === 409
          ? m.modules_key_taken()
          : errorText(e, m.modules_change_failed());
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
      templateError = m.modules_template_required();
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
      templateError = errorText(e, m.modules_create_failed());
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
      <p class="text-muted-foreground">{m.modules_items_empty()}</p>
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

    {#if canCreate}
      <CreateForm
        id="new-item"
        label={m.modules_new_item_title()}
        placeholder={m.modules_new_item_placeholder()}
        submitLabel={m.modules_create_item()}
        create={async (requestId: string, title: string) => {
          const created = await api.createItem(current.id, requestId, title);
          router.navigate(itemPath(workspace.id, created.id));
        }}
      />
      <p class="text-sm text-muted-foreground">{m.modules_new_item_hint()}</p>
      {#if templates.length > 0}
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
            <select
              id="template-source"
              class="h-9 rounded-md border border-input bg-transparent px-3 text-base"
              bind:value={templateId}
            >
              {#each templates as t (t.id)}<option value={t.id}>{t.title}</option>{/each}
            </select>
          </div>
          <div class="flex min-w-40 flex-1 flex-col gap-1">
            <Label for="template-title">{m.modules_template_item_title()}</Label>
            <Input id="template-title" bind:value={templateTitle} maxlength={200} />
          </div>
          <Button type="submit" variant="outline">{m.modules_from_template()}</Button>
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
        aria-label={m.modules_settings()}
      >
        <div class="flex min-w-40 flex-1 flex-col gap-1">
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
          <textarea
            id="module-edit-description"
            bind:value={editDescription}
            maxlength={2000}
            rows={3}
            class="rounded-md border border-input bg-transparent px-3 py-2 text-base shadow-xs focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          ></textarea>
        </div>
        <Button type="submit" variant="outline">{m.common_save()}</Button>
        <Button type="button" variant="outline" onclick={() => void toggleArchive()}
          >{current.archived ? m.modules_unarchive() : m.modules_archive()}</Button
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
