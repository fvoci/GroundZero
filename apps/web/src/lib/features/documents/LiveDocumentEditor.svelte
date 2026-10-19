<script lang="ts">
  // 실시간 공동 편집 화면(D-27 워크스페이스별 ON, D-70). 본문은 Yjs로 바로 함께 고치고(저장 단추·초안 없음),
  // 서버가 조용할 때·마지막 사람이 나갈 때 리비전을 남긴다. 제목은 저장 API로 따로 바꾼다.
  import { onDestroy, untrack } from 'svelte';

  import type { DocumentView } from '$lib/api/client';
  import { api, ApiError, fileContentUrl, newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { uploadFile } from '$lib/features/files/upload';
  import type { ReferenceContext } from '$lib/features/references/context';
  import { me } from '$lib/me.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  import type { LiveSession, LiveStatus, LiveStop } from './live';
  import { closeLive, colorFor, openLive, stopFor } from './live';
  import TiptapEditor from './TiptapEditor.svelte';

  interface Props {
    initial: DocumentView;
    references?: ReferenceContext;
    onsaved?: (doc: DocumentView) => void;
    /** 실시간이 꺼졌다. 부모가 문서를 다시 읽어 일반 편집으로 돌아간다. */
    onrealtimeoff?: () => void;
  }

  const { initial, references, onsaved, onrealtimeoff }: Props = $props();
  const start = untrack(() => initial);
  const INLINE_IMAGES = ['image/png', 'image/jpeg', 'image/gif', 'image/webp'];

  let session = $state<LiveSession | undefined>(undefined);
  let epoch = $state(0);
  let status = $state<LiveStatus>('connecting');
  let stopped = $state<LiveStop | undefined>(undefined);
  let title = $state(start.title);
  let savedTitle = $state(start.title);
  let titleMessage = $state<string | undefined>(undefined);
  let savingTitle = $state(false);

  function open(): void {
    const next = openLive(start.id, {
      name: me.displayName === '' ? m.documents_live_anonymous() : me.displayName,
      color: colorFor(me.userId),
    });
    next.provider.on('status', (event: { status: LiveStatus }) => {
      status = event.status;
    });
    next.provider.on('connection-close', (event: CloseEvent | null) => {
      const stop = event === null ? undefined : stopFor(event.code);
      if (stop === undefined) {
        if (event?.code === 1006) void checkRefusal(next);
        return;
      }
      next.provider.disconnect();
      if (stop === 'reload') {
        // 문서가 다른 길(복원 등)로 바뀌었다. 옛 내용을 섞지 않도록 새 문서로 다시 연다.
        restart();
      } else if (stop === 'off') {
        onrealtimeoff?.();
      } else {
        stopped = stop;
      }
    });
    session = next;
  }

  // 다시 붙기를 서버가 연결 전에 거절하면(권한·세션이 끝남, 실시간이 꺼짐) 브라우저는 이유 없이 1006만 받고
  // y-websocket은 끝없이 다시 붙는다(그동안 고친 내용은 어디에도 남지 않는다). 그때 문서를 다시 읽어 이유를 찾는다.
  // 네트워크 오류면 그대로 다시 붙기를 기다린다.
  let checking = false;
  async function checkRefusal(current: LiveSession): Promise<void> {
    if (checking) return;
    checking = true;
    try {
      const doc = await api.document(start.id);
      if (session !== current || doc.realtime) return;
      current.provider.disconnect();
      onrealtimeoff?.();
    } catch (e: unknown) {
      if (session !== current || !(e instanceof ApiError)) return;
      if (e.status === 401 || e.status === 403 || e.status === 404) {
        current.provider.disconnect();
        stopped = 'denied';
      }
    } finally {
      checking = false;
    }
  }

  function restart(): void {
    if (session !== undefined) closeLive(session);
    session = undefined;
    stopped = undefined;
    epoch += 1;
    open();
  }

  open();
  onDestroy(() => {
    if (session !== undefined) closeLive(session);
  });

  /** 본문 첨부로 올리고 넣을 주소를 돌려준다(일반 편집과 같다). */
  async function insertFile(file: File): Promise<{ href: string; name: string; image: boolean }> {
    const done = await uploadFile(file, { target: { kind: 'document', id: start.id } });
    if (done.file_id === null) throw new Error(done.failure ?? m.documents_upload_failed());
    const image = INLINE_IMAGES.includes(file.type);
    return {
      href: fileContentUrl(done.file_id, done.version ?? undefined, image),
      name: file.name,
      image,
    };
  }

  /** 제목만 바꾼다. 본문은 서버의 최신 본문을 그대로 보낸다(실시간 중 저장 API는 본문을 바꾸지 않는다). */
  async function saveTitle(): Promise<void> {
    const next = title.trim();
    if (next === '') {
      titleMessage = m.documents_title_required();
      return;
    }
    if (next === savedTitle) return;
    savingTitle = true;
    titleMessage = undefined;
    try {
      for (let tries = 0; tries < 2; tries += 1) {
        const current = await api.document(start.id);
        const result = await api.saveDocument(start.id, {
          requestId: newRequestId(),
          baseRevision: current.revision,
          title: next,
          body: current.body,
        });
        if (result.kind === 'saved') {
          savedTitle = result.document.title;
          onsaved?.(result.document);
          return;
        }
      }
      titleMessage = m.documents_title_conflict();
    } catch (e: unknown) {
      titleMessage = errorText(e, m.documents_title_save_failed());
    } finally {
      savingTitle = false;
    }
  }

  function statusText(s: LiveStatus): string {
    switch (s) {
      case 'connected':
        return m.documents_live_connected();
      case 'connecting':
        return m.documents_live_connecting();
      case 'disconnected':
        return m.documents_live_disconnected();
    }
  }

  function stopText(s: LiveStop): string {
    switch (s) {
      case 'denied':
        return m.documents_live_denied();
      case 'invalid':
        return m.documents_live_invalid();
      case 'reload':
        return m.documents_live_reload();
      case 'off':
        return m.documents_live_off();
    }
  }
</script>

<section class="flex flex-col gap-3" aria-label={m.documents_live_label()}>
  <div class="flex flex-wrap items-center gap-2 text-sm">
    <span class="rounded bg-muted px-2 py-0.5 font-mono">{start.reference ?? ''}</span>
    <span role="status" aria-live="polite" class="text-muted-foreground"
      >{stopped === undefined ? statusText(status) : stopText(stopped)}</span
    >
    {#if stopped !== undefined}
      <Button size="sm" variant="outline" onclick={restart}>{m.documents_live_reconnect()}</Button>
    {/if}
  </div>
  <form
    class="flex flex-wrap items-end gap-2"
    onsubmit={(e: SubmitEvent) => {
      e.preventDefault();
      void saveTitle();
    }}
  >
    <div class="flex min-w-0 flex-1 flex-col gap-1">
      <Label for="live-title">{m.documents_title_label()}</Label>
      <Input
        id="live-title"
        bind:value={title}
        readonly={!start.can_edit}
        onblur={() => void saveTitle()}
      />
    </div>
    {#if start.can_edit}
      <Button
        type="submit"
        size="sm"
        variant="outline"
        disabled={savingTitle || title.trim() === savedTitle}>{m.documents_title_save()}</Button
      >
    {/if}
  </form>
  {#if titleMessage !== undefined}<p role="alert" class="text-sm text-destructive">
      {titleMessage}
    </p>{/if}
  {#if session !== undefined}
    {#key epoch}
      <TiptapEditor
        content={start.body}
        editable={start.can_edit && stopped === undefined}
        label={m.documents_body_label()}
        {references}
        insertFile={start.can_edit ? insertFile : undefined}
        collab={session}
      />
    {/key}
  {/if}
  <p class="text-xs text-muted-foreground">
    {m.documents_live_hint()}
  </p>
</section>
