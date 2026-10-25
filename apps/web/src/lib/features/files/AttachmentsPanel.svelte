<script lang="ts">
  // 문서·항목의 첨부 파일(D-06, D-35). 조각 업로드(끊기면 같은 파일을 다시 고르면 이어짐), 새 버전, 이전 버전
  // 내려받기, 이름 바꾸기, 휴지통. 권한은 서버가 판단하고 화면은 가능한 동작만 보인다.
  import { untrack } from 'svelte';

  import type { FileTarget, FileVersionView, FileView } from '$lib/api/client';
  import { api, fileContentUrl } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { formatInstant } from '$lib/display';
  import { me } from '$lib/me.svelte';
  import { m } from '$lib/paraglide/messages.js';
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
    /** 문서의 access=owner 또는 allow_copy. 항목 첨부는 서버 can_download만 따른다. */
    allowCopy?: boolean;
  }

  const { target, canWrite, allowCopy = true }: Props = $props();
  // 올리기를 시작한 화면의 사용자(서버가 세션과 비교한다, #60). 만료된 업로드를 스스로 다시 시작할 때 다른 탭에서 바꾼
  // 계정으로 만들지 않게 한다.
  const uploader = untrack(() => me.userId);

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
  let progress = $state<{ name: string; percent: number } | undefined>(undefined);
  let versions = $state<{ id: string; list: FileVersionView[] } | undefined>(undefined);
  // `opened`: 칸을 열 때마다 새로 매기는 번호. 같은 파일의 칸을 닫았다가 다시 열어도 앞의 칸과 구별한다.
  let renaming = $state<{ id: string; name: string; opened: number } | undefined>(undefined);
  let renameOpened = 0;
  // 이름 바꾸기 실패는 입력 칸 옆에 보이고 입력은 남긴다(D-82).
  let renameError = $state<string | undefined>(undefined);
  let renameBusy = $state(false);
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
        if (mine === seq) error = errorText(e, m.files_load_failed());
      });
  });

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
          expectedUser: uploader,
          ...(fileId === undefined ? { target } : { fileId }),
          signal: cancel.signal,
          onProgress: (p) => {
            progress = {
              name: file.name,
              percent: p.total === 0 ? 100 : Math.floor((p.sent / p.total) * 100),
            };
          },
        });
        notifyDone(
          done.version !== null && done.version > 1
            ? m.files_uploaded_version({ name: file.name, version: done.version })
            : m.files_uploaded({ name: file.name }),
        );
      } catch (e: unknown) {
        error = cancel.signal.aborted
          ? m.files_upload_cancelled({ name: file.name })
          : m.files_named_error({
              name: file.name,
              error: errorText(e, m.files_upload_failed()),
            });
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
      error = errorText(e, m.files_versions_load_failed());
    }
  }

  async function act(run: () => Promise<unknown>, done: string): Promise<void> {
    error = undefined;
    try {
      await run();
      notifyDone(done);
      reloadKey += 1;
    } catch (e: unknown) {
      error = errorText(e, m.files_action_failed());
    }
  }

  /**
   * 입력 칸은 서버가 받아들인 뒤에만 닫는다. 거절되면 친 이름과 이유가 칸에 남는다.
   * 응답 전에 칸을 닫았거나 다른 칸을 열었으면(같은 파일의 칸을 다시 열었어도) 그 칸은 건드리지 않는다. 새 칸에 친
   * 이름은 남고, 실패는 파일 이름과 함께 패널 상태 줄에 보인다.
   */
  async function rename(file: FileView, name: string, opened: number): Promise<void> {
    if (renameBusy) return;
    renameBusy = true;
    renameError = undefined;
    error = undefined;
    try {
      await api.renameFile(file.id, name);
      if (renaming?.opened === opened) renaming = undefined;
      notifyDone(m.files_renamed({ name }));
      reloadKey += 1;
    } catch (e: unknown) {
      const reason = errorText(e, m.files_action_failed());
      if (renaming?.opened === opened) renameError = reason;
      else error = m.files_named_error({ name: file.name, error: reason });
    } finally {
      renameBusy = false;
    }
  }

  const when = (iso: string): string => formatInstant(iso);
</script>

<section class="flex flex-col gap-3" aria-labelledby="attachments-heading">
  <div class="flex items-center gap-2">
    <h2 id="attachments-heading" class="text-lg font-semibold">{m.files_heading()}</h2>
    {#if canWrite}
      <Button
        size="sm"
        variant="outline"
        onclick={() => {
          choose();
        }}
        disabled={progress !== undefined}>{m.files_upload()}</Button
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
  <p role="status" aria-live="polite" class="text-sm empty:hidden">
    {#if progress !== undefined}
      {m.files_uploading({ name: progress.name, percent: progress.percent })}
      <Button size="sm" variant="ghost" onclick={() => controller?.abort()}
        >{m.common_cancel()}</Button
      >
    {:else if error !== undefined}
      <span class="text-destructive">{error}</span>
    {/if}
  </p>
  {#if files.length === 0}
    <p class="text-sm text-muted-foreground">{m.files_empty()}</p>
  {:else}
    <ul class="flex flex-col gap-2" data-testid="attachments">
      {#each files as f (f.id)}
        <li class="flex flex-col gap-1 rounded-md border p-2 text-sm">
          <div class="flex flex-wrap items-center gap-2">
            {#if renaming?.id === f.id}
              <form
                class="flex flex-1 flex-wrap gap-2"
                onsubmit={(e: SubmitEvent) => {
                  e.preventDefault();
                  if (renaming !== undefined) void rename(f, renaming.name, renaming.opened);
                }}
              >
                <Input
                  class="min-w-0 flex-1"
                  aria-label={m.files_new_name()}
                  bind:value={renaming.name}
                  maxlength={255}
                  aria-invalid={renameError !== undefined}
                  aria-describedby={renameError !== undefined ? `rename-error-${f.id}` : undefined}
                />
                <Button size="sm" type="submit" disabled={renameBusy}>{m.common_save()}</Button>
                <Button
                  size="sm"
                  variant="ghost"
                  onclick={() => {
                    renaming = undefined;
                    renameError = undefined;
                  }}>{m.common_cancel()}</Button
                >
                {#if renameError !== undefined}
                  <p
                    id={`rename-error-${f.id}`}
                    role="alert"
                    class="w-full text-sm text-destructive"
                  >
                    {renameError}
                  </p>
                {/if}
              </form>
            {:else}
              {#if allowCopy && f.can_download}
                <a
                  class="min-w-0 flex-1 truncate font-medium underline-offset-2 hover:underline"
                  title={f.name}
                  href={fileContentUrl(f.id)}>{f.name}</a
                >
              {:else}
                <span class="min-w-0 flex-1 truncate font-medium" title={m.files_download_blocked()}
                  >{f.name}</span
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
                aria-label={m.files_preview_aria({ name: f.name })}>{m.files_preview()}</Button
              >
            {:else if allowCopy && f.can_download && officeKind(f.content_type, f.name) !== undefined}
              {@const kind = officeKind(f.content_type, f.name)}
              <Button
                size="sm"
                variant="ghost"
                aria-label={m.files_preview_aria({ name: f.name })}
                aria-expanded={office?.id === f.id}
                onclick={() => {
                  office =
                    office?.id === f.id || kind === undefined
                      ? undefined
                      : { id: f.id, name: f.name, size: f.size, kind };
                }}>{m.files_preview()}</Button
              >
            {/if}
            <Button
              size="sm"
              variant="ghost"
              aria-label={m.files_versions_aria({ name: f.name })}
              aria-expanded={versions?.id === f.id}
              onclick={() => {
                if (versions?.id === f.id) versions = undefined;
                else void showVersions(f.id);
              }}>{m.files_versions()}</Button
            >
            {#if f.can_write}
              <Button
                size="sm"
                variant="ghost"
                aria-label={m.files_new_version_aria({ name: f.name })}
                onclick={() => {
                  choose(f.id);
                }}
                disabled={progress !== undefined}>{m.files_new_version()}</Button
              >
              <Button
                size="sm"
                variant="ghost"
                aria-label={m.files_rename_aria({ name: f.name })}
                onclick={() => {
                  renaming = { id: f.id, name: f.name, opened: ++renameOpened };
                  renameError = undefined;
                }}>{m.files_rename()}</Button
              >
              <Button
                size="sm"
                variant="ghost"
                aria-label={m.files_trash_aria({ name: f.name })}
                onclick={() =>
                  void act(() => api.trashFile(f.id), m.files_trashed({ name: f.name }))}
                >{m.files_trash()}</Button
              >
            {/if}
          </div>
          {#if versions?.id === f.id}
            <ol
              class="flex flex-col gap-1 border-t pt-2"
              aria-label={m.files_version_list_aria({ name: f.name })}
            >
              {#each versions.list as v (v.version)}
                <li class="flex flex-wrap items-baseline gap-2">
                  {#if allowCopy && f.can_download}
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
                      aria-label={m.files_delete_version_aria({ name: f.name, version: v.version })}
                      onclick={() =>
                        void act(
                          () => api.deleteFileVersion(f.id, v.version),
                          m.files_version_deleted({ version: v.version, name: f.name }),
                        ).then(() => showVersions(f.id))}>{m.files_delete()}</Button
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
