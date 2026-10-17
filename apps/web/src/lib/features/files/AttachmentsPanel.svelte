<script lang="ts">
  // 문서·항목의 첨부 파일(D-06, D-35). 조각 업로드(끊기면 같은 파일을 다시 고르면 이어짐), 새 버전, 이전 버전
  // 내려받기, 이름 바꾸기, 휴지통. 권한은 서버가 판단하고 화면은 가능한 동작만 보인다.
  import type { FileTarget, FileVersionView, FileView } from '$lib/api/client';
  import { api, ApiError, fileContentUrl } from '$lib/api/client';
  import { localTimeZone, wallTime } from '$lib/features/events/time';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';

  import type { OfficeKind } from './office';
  import { officeKind } from './office';
  import OfficePreview from './OfficePreview.svelte';
  import { formatBytes, uploadFile } from './upload';

  interface Props {
    target: FileTarget;
    /** 새 파일을 붙일 수 있는지(대상에 쓸 수 있는지). */
    canWrite: boolean;
  }

  const { target, canWrite }: Props = $props();

  const zone = localTimeZone();
  const INLINE = new Set([
    'image/png',
    'image/jpeg',
    'image/gif',
    'image/webp',
    'application/pdf',
    'text/plain',
  ]);

  let files = $state<FileView[]>([]);
  let error = $state<string | undefined>(undefined);
  let status = $state('');
  let progress = $state<{ name: string; percent: number } | undefined>(undefined);
  let versions = $state<{ id: string; list: FileVersionView[] } | undefined>(undefined);
  let renaming = $state<{ id: string; name: string } | undefined>(undefined);
  let reloadKey = $state(0);
  // 열려 있는 Office 미리보기(DOCX·XLSX, D-71).
  let office = $state<{ id: string; name: string; size: number; kind: OfficeKind } | undefined>(
    undefined,
  );
  let picker = $state<HTMLInputElement | undefined>(undefined);
  let versionFor: string | undefined;
  let controller: AbortController | undefined;
  let seq = 0;

  $effect(() => {
    const { kind, id } = target;
    if (reloadKey < 0) return;
    const mine = ++seq;
    (kind === 'document' ? api.documentFiles(id) : api.itemFiles(id))
      .then((found) => {
        if (mine === seq) files = found;
      })
      .catch((e: unknown) => {
        if (mine === seq)
          error = e instanceof ApiError ? e.message : '첨부 파일을 불러오지 못했습니다.';
      });
  });

  function message(e: unknown, fallback: string): string {
    return e instanceof ApiError ? e.message : fallback;
  }

  async function send(chosen: FileList | null): Promise<void> {
    if (chosen === null || chosen.length === 0) return;
    const fileId = versionFor;
    versionFor = undefined;
    error = undefined;
    const cancel = new AbortController();
    controller = cancel;
    for (const file of Array.from(chosen)) {
      progress = { name: file.name, percent: 0 };
      try {
        const done = await uploadFile(file, {
          ...(fileId === undefined ? { target } : { fileId }),
          signal: cancel.signal,
          onProgress: (p) => {
            progress = {
              name: file.name,
              percent: p.total === 0 ? 100 : Math.floor((p.sent / p.total) * 100),
            };
          },
        });
        status = `올렸습니다: ${file.name}${done.version !== null && done.version > 1 ? ` (v${String(done.version)})` : ''}`;
      } catch (e: unknown) {
        error = cancel.signal.aborted
          ? `취소했습니다: ${file.name}`
          : `${file.name}: ${message(e, '올리지 못했습니다. 같은 파일을 다시 고르면 이어서 올립니다.')}`;
        break;
      }
    }
    controller = undefined;
    progress = undefined;
    reloadKey += 1;
    if (versions !== undefined) await showVersions(versions.id);
  }

  function choose(fileId?: string): void {
    versionFor = fileId;
    if (picker !== undefined) {
      picker.value = '';
      picker.multiple = fileId === undefined;
      picker.click();
    }
  }

  async function showVersions(id: string): Promise<void> {
    try {
      const detail = await api.file(id);
      versions = { id, list: detail.versions };
    } catch (e: unknown) {
      error = message(e, '버전 기록을 불러오지 못했습니다.');
    }
  }

  async function act(run: () => Promise<unknown>, done: string): Promise<void> {
    error = undefined;
    try {
      await run();
      status = done;
      reloadKey += 1;
    } catch (e: unknown) {
      error = message(e, '처리하지 못했습니다.');
    }
  }

  const when = (iso: string): string => wallTime(iso, zone).replace('T', ' ');
</script>

<section class="flex flex-col gap-3" aria-labelledby="attachments-heading">
  <div class="flex items-center gap-2">
    <h2 id="attachments-heading" class="text-lg font-semibold">첨부 파일</h2>
    {#if canWrite}
      <Button
        size="sm"
        variant="outline"
        onclick={() => {
          choose();
        }}
        disabled={progress !== undefined}>파일 올리기</Button
      >
    {/if}
    <input
      bind:this={picker}
      type="file"
      class="hidden"
      aria-hidden="true"
      tabindex="-1"
      onchange={(e: Event) => {
        if (e.currentTarget instanceof HTMLInputElement) void send(e.currentTarget.files);
      }}
    />
  </div>
  <p aria-live="polite" class="text-sm">
    {#if progress !== undefined}
      {progress.name} 올리는 중… {progress.percent}%
      <Button size="sm" variant="ghost" onclick={() => controller?.abort()}>취소</Button>
    {:else if error !== undefined}
      <span class="text-destructive">{error}</span>
    {:else}
      {status}
    {/if}
  </p>
  {#if files.length === 0}
    <p class="text-sm text-muted-foreground">첨부 파일이 없습니다.</p>
  {:else}
    <ul class="flex flex-col gap-2" data-testid="attachments">
      {#each files as f (f.id)}
        <li class="flex flex-col gap-1 rounded-md border p-2 text-sm">
          <div class="flex flex-wrap items-center gap-2">
            {#if renaming?.id === f.id}
              <form
                class="flex flex-1 gap-2"
                onsubmit={(e: SubmitEvent) => {
                  e.preventDefault();
                  const next = renaming;
                  renaming = undefined;
                  if (next !== undefined)
                    void act(
                      () => api.renameFile(next.id, next.name),
                      `이름을 바꿨습니다: ${next.name}`,
                    );
                }}
              >
                <Input aria-label="새 이름" bind:value={renaming.name} maxlength={255} />
                <Button size="sm" type="submit">저장</Button>
                <Button size="sm" variant="ghost" onclick={() => (renaming = undefined)}
                  >취소</Button
                >
              </form>
            {:else}
              {#if f.can_download}
                <a
                  class="min-w-0 flex-1 truncate font-medium underline-offset-2 hover:underline"
                  href={fileContentUrl(f.id)}>{f.name}</a
                >
              {:else}
                <span
                  class="min-w-0 flex-1 truncate font-medium"
                  title="내려받기가 허용되지 않은 문서의 첨부">{f.name}</span
                >
              {/if}
              <span class="text-xs text-muted-foreground"
                >v{f.current_version} · {formatBytes(f.size)} · {when(f.updated_at)}</span
              >
            {/if}
          </div>
          <div class="flex flex-wrap gap-1">
            {#if INLINE.has(f.content_type)}
              <Button
                size="sm"
                variant="ghost"
                href={fileContentUrl(f.id, undefined, true)}
                target="_blank"
                rel="noopener"
                aria-label={`${f.name} 미리보기`}>미리보기</Button
              >
            {:else if f.can_download && officeKind(f.content_type, f.name) !== undefined}
              {@const kind = officeKind(f.content_type, f.name)}
              <Button
                size="sm"
                variant="ghost"
                aria-label={`${f.name} 미리보기`}
                aria-expanded={office?.id === f.id}
                onclick={() => {
                  office =
                    office?.id === f.id || kind === undefined
                      ? undefined
                      : { id: f.id, name: f.name, size: f.size, kind };
                }}>미리보기</Button
              >
            {/if}
            <Button
              size="sm"
              variant="ghost"
              aria-label={`${f.name} 버전 기록`}
              aria-expanded={versions?.id === f.id}
              onclick={() => {
                if (versions?.id === f.id) versions = undefined;
                else void showVersions(f.id);
              }}>버전 기록</Button
            >
            {#if f.can_write}
              <Button
                size="sm"
                variant="ghost"
                aria-label={`${f.name} 새 버전 올리기`}
                onclick={() => {
                  choose(f.id);
                }}
                disabled={progress !== undefined}>새 버전</Button
              >
              <Button
                size="sm"
                variant="ghost"
                aria-label={`${f.name} 이름 바꾸기`}
                onclick={() => (renaming = { id: f.id, name: f.name })}>이름 바꾸기</Button
              >
              <Button
                size="sm"
                variant="ghost"
                aria-label={`${f.name} 휴지통으로`}
                onclick={() =>
                  void act(() => api.trashFile(f.id), `휴지통으로 옮겼습니다: ${f.name}`)}
                >휴지통으로</Button
              >
            {/if}
          </div>
          {#if versions?.id === f.id}
            <ol class="flex flex-col gap-1 border-t pt-2" aria-label={`${f.name} 버전`}>
              {#each versions.list as v (v.version)}
                <li class="flex flex-wrap items-baseline gap-2">
                  {#if f.can_download}
                    <a
                      class="underline-offset-2 hover:underline"
                      href={fileContentUrl(f.id, v.version)}>v{v.version}</a
                    >
                  {:else}
                    <span>v{v.version}</span>
                  {/if}
                  <span class="text-xs text-muted-foreground"
                    >{formatBytes(v.size)} · {v.uploaded_by_name ?? ''} · {when(v.created_at)}</span
                  >
                  {#if v.note !== ''}<span>{v.note}</span>{/if}
                  {#if f.can_write && v.version !== f.current_version}
                    <Button
                      size="sm"
                      variant="ghost"
                      aria-label={`${f.name} v${String(v.version)} 지우기`}
                      onclick={() =>
                        void act(
                          () => api.deleteFileVersion(f.id, v.version),
                          `v${String(v.version)}을 지웠습니다: ${f.name}`,
                        ).then(() => showVersions(f.id))}>지우기</Button
                    >
                  {/if}
                </li>
              {/each}
            </ol>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
  {#if office !== undefined}
    {#key office.id}
      <OfficePreview
        fileId={office.id}
        name={office.name}
        size={office.size}
        kind={office.kind}
        onclose={() => {
          office = undefined;
        }}
      />
    {/key}
  {/if}
</section>
