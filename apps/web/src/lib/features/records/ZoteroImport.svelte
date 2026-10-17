<script lang="ts">
  // Zotero에서 문헌 가져오기(D-34 읽기 연동, X-05 PDF·주석·양방향 쓰기 제외). API 키는 이 화면의 입력에만 있고
  // 저장하지 않는다(서버도 저장하지 않음). 같은 문헌을 다시 가져오면 서지만 새로 하고 내 칸은 둔다.
  import type { ZoteroLibraryInput } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  interface Props {
    itemId: string;
    onimported: () => void;
  }

  const { itemId, onimported }: Props = $props();

  let open = $state(false);
  let libraryType = $state<'user' | 'group'>('user');
  let libraryId = $state('');
  let apiKey = $state('');
  let collections = $state<{ key: string; name: string }[]>([]);
  let collection = $state('');
  let busy = $state(false);
  let error = $state<string | undefined>(undefined);
  let status = $state('');
  const box = 'h-9 rounded-md border border-input bg-transparent px-3 text-base';

  function library(): ZoteroLibraryInput | undefined {
    if (apiKey.trim() === '' || !/^\d+$/.test(libraryId.trim())) {
      error = m.records_zotero_invalid();
      return undefined;
    }
    error = undefined;
    return { api_key: apiKey.trim(), library_type: libraryType, library_id: libraryId.trim() };
  }

  async function loadCollections(): Promise<void> {
    const lib = library();
    if (lib === undefined) return;
    busy = true;
    try {
      collections = await api.zoteroCollections(lib);
      status = m.records_zotero_collections_read({ count: collections.length });
    } catch (e: unknown) {
      error = errorText(e, m.records_zotero_unreachable());
    } finally {
      busy = false;
    }
  }

  async function run(): Promise<void> {
    const lib = library();
    if (lib === undefined) return;
    busy = true;
    try {
      const r = await api.zoteroImport(itemId, lib, collection === '' ? null : collection);
      status = m.records_zotero_result({
        created: r.created,
        updated: r.updated,
        skipped: r.skipped,
      });
      onimported();
    } catch (e: unknown) {
      error = errorText(e, m.records_zotero_failed());
    } finally {
      busy = false;
    }
  }
</script>

<div class="flex flex-col gap-2 text-sm">
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
        <select id="zotero-type" class={box} bind:value={libraryType}>
          <option value="user">{m.records_zotero_my_library()}</option>
          <option value="group">{m.records_zotero_group()}</option>
        </select>
      </div>
      <div class="flex w-32 flex-col gap-1">
        <Label for="zotero-id"
          >{libraryType === 'user'
            ? m.records_zotero_user_id()
            : m.records_zotero_group_id()}</Label
        >
        <Input id="zotero-id" inputmode="numeric" bind:value={libraryId} />
      </div>
      <div class="flex min-w-48 flex-1 flex-col gap-1">
        <Label for="zotero-key">{m.records_zotero_key()}</Label>
        <Input id="zotero-key" type="password" autocomplete="off" bind:value={apiKey} />
      </div>
      <Button
        type="button"
        size="sm"
        variant="ghost"
        disabled={busy}
        onclick={() => void loadCollections()}>{m.records_zotero_read_collections()}</Button
      >
      {#if collections.length > 0}
        <div class="flex flex-col gap-1">
          <Label for="zotero-collection">{m.records_zotero_collection()}</Label>
          <select id="zotero-collection" class={box} bind:value={collection}>
            <option value="">{m.records_zotero_all()}</option>
            {#each collections as c (c.key)}<option value={c.key}>{c.name}</option>{/each}
          </select>
        </div>
      {/if}
      <Button type="submit" size="sm" disabled={busy}
        >{busy ? m.records_zotero_importing() : m.records_zotero_run()}</Button
      >
      <Button
        type="button"
        size="sm"
        variant="ghost"
        onclick={() => {
          open = false;
          apiKey = '';
        }}>{m.records_zotero_close()}</Button
      >
      <p class="w-full text-muted-foreground">{m.records_zotero_hint()}</p>
    </form>
  {/if}
  {#if error !== undefined}<p role="alert" class="text-destructive">{error}</p>{/if}
  <p aria-live="polite">{status}</p>
</div>
