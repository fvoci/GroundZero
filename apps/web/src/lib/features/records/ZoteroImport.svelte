<script lang="ts">
  // Zotero에서 문헌 가져오기(D-34 읽기 연동, X-05 PDF·주석·양방향 쓰기 제외). API 키는 이 화면의 입력에만 있고
  // 저장하지 않는다(서버도 저장하지 않음). 같은 문헌을 다시 가져오면 서지만 새로 하고 내 칸은 둔다.
  import type { ZoteroLibraryInput } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
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
      error = 'API 키와 숫자로 된 라이브러리 ID를 적어 주세요.';
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
      status = `컬렉션 ${String(collections.length)}개를 읽었습니다.`;
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : 'Zotero에 닿지 못했습니다.';
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
      status = `새 문헌 ${String(r.created)}개, 고친 문헌 ${String(r.updated)}개(건너뜀 ${String(r.skipped)}개).`;
      onimported();
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '가져오지 못했습니다.';
    } finally {
      busy = false;
    }
  }
</script>

<div class="flex flex-col gap-2 text-sm">
  {#if !open}
    <div>
      <Button size="sm" variant="outline" onclick={() => (open = true)}>Zotero에서 가져오기</Button>
    </div>
  {:else}
    <form
      class="flex flex-wrap items-end gap-2 rounded-md border p-2"
      aria-label="Zotero에서 가져오기"
      onsubmit={(e: SubmitEvent) => {
        e.preventDefault();
        void run();
      }}
    >
      <div class="flex flex-col gap-1">
        <Label for="zotero-type">라이브러리</Label>
        <select id="zotero-type" class={box} bind:value={libraryType}>
          <option value="user">내 라이브러리</option>
          <option value="group">그룹</option>
        </select>
      </div>
      <div class="flex w-32 flex-col gap-1">
        <Label for="zotero-id">{libraryType === 'user' ? '사용자 ID' : '그룹 ID'}</Label>
        <Input id="zotero-id" inputmode="numeric" bind:value={libraryId} />
      </div>
      <div class="flex min-w-48 flex-1 flex-col gap-1">
        <Label for="zotero-key">API 키(저장하지 않음)</Label>
        <Input id="zotero-key" type="password" autocomplete="off" bind:value={apiKey} />
      </div>
      <Button
        type="button"
        size="sm"
        variant="ghost"
        disabled={busy}
        onclick={() => void loadCollections()}>컬렉션 읽기</Button
      >
      {#if collections.length > 0}
        <div class="flex flex-col gap-1">
          <Label for="zotero-collection">컬렉션</Label>
          <select id="zotero-collection" class={box} bind:value={collection}>
            <option value="">전체(최상위 문헌)</option>
            {#each collections as c (c.key)}<option value={c.key}>{c.name}</option>{/each}
          </select>
        </div>
      {/if}
      <Button type="submit" size="sm" disabled={busy}>{busy ? '가져오는 중…' : '가져오기'}</Button>
      <Button
        type="button"
        size="sm"
        variant="ghost"
        onclick={() => {
          open = false;
          apiKey = '';
        }}>닫기</Button
      >
      <p class="w-full text-muted-foreground">
        서지 정보·컬렉션만 읽습니다(PDF·주석은 가져오지 않고 Zotero에 쓰지 않습니다). 키는 Zotero
        설정의 읽기용 키를 쓰세요. 이 화면을 닫으면 지워집니다.
      </p>
    </form>
  {/if}
  {#if error !== undefined}<p role="alert" class="text-destructive">{error}</p>{/if}
  <p aria-live="polite">{status}</p>
</div>
