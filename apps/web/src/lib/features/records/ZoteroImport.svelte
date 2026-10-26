<script lang="ts">
  // Zotero에서 문헌 가져오기(D-34 읽기 연동, X-05 PDF·주석·양방향 쓰기 제외). API 키는 이 화면의 입력에만 있고
  // 저장하지 않는다(서버도 저장하지 않음). 같은 문헌을 다시 가져오면 서지만 새로 하고 내 칸은 둔다.
  import type { ZoteroLibraryInput } from '$lib/api/client';
  import { onMount, untrack } from 'svelte';
  import { me } from '$lib/me.svelte';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { api } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import { NativeSelect } from '$lib/ui/native-select';

  interface Props {
    itemId: string;
    onimported: () => void;
  }

  const { itemId, onimported }: Props = $props();

  let open = $state(false);
  let libraryType = $state<'user' | 'group'>('user');
  let libraryId = $state('');
  let apiKey = $state('');
  let keyVersion = $state(0);
  let collections = $state<{ key: string; name: string }[]>([]);
  let collection = $state('');
  let busy = $state(false);
  let error = $state<string | undefined>(undefined);
  let status = $state('');
  // Only these non-secret inputs cross the durable draft boundary. Keep the ID verbatim.
  interface ImportSettings {
    libraryType: 'user' | 'group';
    libraryId: string;
    collection: string;
  }
  const snapshot = (): ImportSettings => ({ libraryType, libraryId, collection });
  // This verification belongs only to the current screen, never to a stored draft.
  let verified = $state<{ settings: string; keyVersion: number }>();
  const collectionsCurrent = $derived(
    verified?.settings === JSON.stringify({ libraryType, libraryId }) &&
      verified.keyVersion === keyVersion,
  );
  const collectionValid = $derived(
    collection === '' || (collectionsCurrent && collections.some((c) => c.key === collection)),
  );
  const draft = createFormDraft<ImportSettings>({
    name: () => `zotero-import:${itemId}`,
    snapshot,
    parse: (raw) => {
      if (
        typeof raw !== 'object' ||
        raw === null ||
        !('libraryType' in raw) ||
        (raw.libraryType !== 'user' && raw.libraryType !== 'group') ||
        !('libraryId' in raw) ||
        typeof raw.libraryId !== 'string' ||
        !('collection' in raw) ||
        typeof raw.collection !== 'string'
      )
        return undefined;
      return { libraryType: raw.libraryType, libraryId: raw.libraryId, collection: raw.collection };
    },
    restore: (value) => {
      libraryType = value.libraryType;
      libraryId = value.libraryId;
      collection = value.collection;
      collections = [];
      verified = undefined;
      open = true;
    },
    reset: () => {
      libraryType = 'user';
      libraryId = '';
      collection = '';
      collections = [];
      verified = undefined;
      busy = false;
      error = undefined;
      status = '';
    },
    dirty: () => libraryType !== 'user' || libraryId !== '' || collection !== '',
    busy: () => busy,
  });
  onMount(draft.attach);
  let credentialOwner: string | undefined;
  $effect(() => {
    // A different owner/target must not inherit the previous screen's credential.
    const owner = JSON.stringify([me.userId, itemId]);
    if (credentialOwner === owner) return;
    credentialOwner = owner;
    untrack(() => {
      apiKey = '';
      keyVersion += 1;
    });
  });

  function library(): ZoteroLibraryInput | undefined {
    if (apiKey.trim() === '' || !/^\d+$/.test(libraryId.trim())) {
      error = m.records_zotero_invalid();
      return undefined;
    }
    error = undefined;
    return { api_key: apiKey.trim(), library_type: libraryType, library_id: libraryId.trim() };
  }

  async function loadCollections(): Promise<void> {
    if (busy) return;
    const attempt = draft.capture();
    if (attempt === undefined) return;
    const lib = library();
    if (lib === undefined) return;
    const context = { settings: JSON.stringify({ libraryType, libraryId }), keyVersion };
    verified = undefined;
    busy = true;
    try {
      const list = await api.zoteroCollections(lib);
      if (!draft.isCurrent(attempt) || context.keyVersion !== keyVersion) return;
      collections = list;
      verified = context;
      status = m.records_zotero_collections_read({ count: collections.length });
    } catch (e: unknown) {
      if (!draft.isCurrent(attempt) || context.keyVersion !== keyVersion) return;
      error = errorText(e, m.records_zotero_unreachable());
    } finally {
      if (draft.isOwner(attempt)) busy = false;
    }
  }

  async function run(): Promise<void> {
    if (busy || !collectionValid) return;
    const attempt = draft.capture();
    if (attempt === undefined) return;
    const lib = library();
    if (lib === undefined) return;
    busy = true;
    try {
      const r = await api.zoteroImport(itemId, lib, collection === '' ? null : collection);
      if (!draft.isOwner(attempt)) return;
      draft.complete(attempt);
      status = m.records_zotero_result({
        created: r.created,
        updated: r.updated,
        skipped: r.skipped,
      });
      onimported();
    } catch (e: unknown) {
      if (!draft.isOwner(attempt)) return;
      error = errorText(e, m.records_zotero_failed());
    } finally {
      if (draft.isOwner(attempt)) busy = false;
    }
  }
</script>

<DiscardChangesConfirm
  open={draft.leave.open}
  onkeep={draft.leave.keep}
  ondiscard={draft.discardAndLeave}
/>
<div class="flex flex-col gap-2 text-sm">
  {#each draft.offers as entry (entry.key)}
    <FormDraftOffer
      onrestore={() => {
        draft.restore(entry);
      }}
      ondiscard={() => {
        draft.discard(entry);
      }}
    />
  {/each}
  {#if !open}
    <div>
      <Button size="sm" variant="outline" onclick={() => (open = true)}
        >{m.records_zotero_import()}</Button
      >
    </div>
  {:else}
    <form
      class="flex flex-wrap items-end gap-2 rounded-md border p-2"
      aria-label={m.records_zotero_import()}
      onsubmit={(e: SubmitEvent) => {
        e.preventDefault();
        void run();
      }}
    >
      <div class="flex flex-col gap-1">
        <Label for="zotero-type">{m.records_zotero_library()}</Label>
        <NativeSelect id="zotero-type" class="w-full" bind:value={libraryType}>
          <option value="user">{m.records_zotero_my_library()}</option>
          <option value="group">{m.records_zotero_group()}</option>
        </NativeSelect>
      </div>
      <div class="flex w-32 flex-col gap-1">
        <Label for="zotero-id"
          >{libraryType === 'user'
            ? m.records_zotero_user_id()
            : m.records_zotero_group_id()}</Label
        >
        <Input id="zotero-id" inputmode="numeric" bind:value={libraryId} />
      </div>
      <div class="flex min-w-[min(12rem,100%)] flex-1 flex-col gap-1">
        <Label for="zotero-key">{m.records_zotero_key()}</Label>
        <Input
          id="zotero-key"
          type="password"
          autocomplete="off"
          bind:value={apiKey}
          oninput={() => {
            keyVersion += 1;
          }}
        />
      </div>
      <Button
        type="button"
        size="sm"
        variant="ghost"
        disabled={busy}
        onclick={() => void loadCollections()}>{m.records_zotero_read_collections()}</Button
      >
      {#if collection !== '' || (collectionsCurrent && collections.length > 0)}
        <div class="flex flex-col gap-1">
          <Label for="zotero-collection">{m.records_zotero_collection()}</Label>
          <NativeSelect id="zotero-collection" class="w-full" bind:value={collection}>
            <option value="">{m.records_zotero_all()}</option>
            {#if !collectionValid}<option value={collection} disabled>{collection}</option>{/if}
            {#if collectionsCurrent}
              {#each collections as c (c.key)}<option value={c.key}>{c.name}</option>{/each}
            {/if}
          </NativeSelect>
        </div>
      {/if}
      <Button
        type="submit"
        size="sm"
        disabled={busy || !collectionValid || !draft.active || draft.blocked}
        >{busy ? m.records_zotero_importing() : m.records_zotero_run()}</Button
      >
      <Button
        type="button"
        size="sm"
        variant="ghost"
        onclick={() => {
          draft.leave.request(() => {
            open = false;
            apiKey = '';
            keyVersion += 1;
          });
        }}>{m.records_zotero_close()}</Button
      >
      <p class="w-full text-muted-foreground">{m.records_zotero_hint()}</p>
    </form>
  {/if}
  {#if error !== undefined}<p role="alert" class="text-destructive">{error}</p>{/if}
  <p aria-live="polite">{status}</p>
</div>
