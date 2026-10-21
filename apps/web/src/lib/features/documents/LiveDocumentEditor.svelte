<script lang="ts">
  // 실시간 공동 편집 화면(D-27 워크스페이스별 ON, D-70). 본문은 Yjs로 바로 함께 고치고(저장 단추 없음),
  // 서버가 조용할 때·마지막 사람이 나갈 때 리비전을 남긴다. 제목은 저장 API로 따로 바꾼다.
  // 서버와 맞춰지지 않은 동안(끊김) 고친 본문은 이 사용자의 초안으로 남겨, 실시간이 꺼지거나 권한이 바뀌거나
  // 새로고침해도 사라지지 않게 한다. 다시 열 때 서버 내용과 다르면 내 노트로 보존할지 묻는다(실시간 문서에 섞지 않음).
  import { formatInstant } from '$lib/display';
  import { onDestroy, untrack } from 'svelte';

  import type { DocNode, DocumentView } from '$lib/api/client';
  import { api, ApiError, fileContentUrl, newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { uploadFile } from '$lib/features/files/upload';
  import type { ReferenceContext } from '$lib/features/references/context';
  import { me } from '$lib/me.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { router, standalonePath } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  import type { Draft } from './drafts';
  import {
    adoptOrphans,
    clearDraft,
    loadOffers,
    takeOrReserve,
    removeOffer,
    withoutApplied,
    storeDraft,
  } from './drafts';
  import type { LiveSession, LiveStatus, LiveStop, UnsyncedWatch } from './live';
  import { closeLive, colorFor, openLive, stopFor, watchUnsynced } from './live';
  import TiptapEditor from './TiptapEditor.svelte';

  interface Props {
    initial: DocumentView;
    references?: ReferenceContext;
    onsaved?: (doc: DocumentView) => void;
    /** 실시간이 꺼졌다. 부모가 문서를 다시 읽어 일반 편집으로 돌아간다. */
    onrealtimeoff?: () => void;
    /** 권한이 바뀌어 닫혔다가 다시 연결한다. 부모가 지금 권한으로 문서를 다시 연다(읽기면 읽기 전용). */
    onreload?: () => void;
  }

  const { initial, references, onsaved, onrealtimeoff, onreload }: Props = $props();
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

  // 서버가 받았다고 확인되지 않은 편집을 초안으로 남긴다. 다시 열 때(또는 방을 새로 열 때) 남아 있던 초안은
  // 고르게 할 줄로 옮겨 하나씩 고르게 하고, 그동안의 새 편집은 지금 편집의 초안으로 따로 남는다(덮지 않음).
  let latest: DocNode | undefined;
  let watch: UnsyncedWatch | undefined;
  // 저장소가 가득 차 줄로 옮기지 못한 지난 초안. 그 자리에 둔 채 고르게 하고, 고를 때까지 이 탭의 새 편집은 그 키에
  // 쓰거나 지우지 않는다(덮어 사라지지 않게).
  let reserved: Draft | undefined = untrack(() => takeOrReserve(start.id).reserved);
  // 닫힌 탭이 남긴 이 문서의 초안을 줄로 넘겨받는다(살아 있는 탭의 것은 두고). 이미 맞춰졌으면 바로 고르게 한다.
  void adoptOrphans(untrack(() => start.id)).then((adopted) => {
    if (adopted > 0 && checkedEarlier && found === undefined) showNextOffer();
  });
  let checkedEarlier = false;
  let found = $state<Draft | undefined>(undefined);
  let foundIsReserved = false;
  let foundMessage = $state<string | undefined>(undefined);
  let keepingFound = $state(false);
  let foundRequest: { note: string; save: string } | undefined;
  let keepTimer: ReturnType<typeof setTimeout> | undefined;

  function keepDraft(): void {
    if (keepTimer !== undefined) clearTimeout(keepTimer);
    keepTimer = undefined;
    if (latest === undefined || reserved !== undefined) return;
    storeDraft(start.id, {
      baseRevision: start.revision,
      title,
      body: latest,
      savedAt: new Date().toISOString(),
    });
  }

  /** 타자마다 쓰지 않도록 조금 모아 남긴다(멈추거나 닫을 때는 바로 남긴다). */
  function keepDraftSoon(): void {
    if (keepTimer !== undefined) clearTimeout(keepTimer);
    keepTimer = setTimeout(keepDraft, 300);
  }

  /** 멈추거나 닫기 전에, 서버가 받았다고 확인되지 않은 편집이 있으면 바로 남긴다. */
  function keepIfDirty(): void {
    if (watch?.dirty === true) keepDraft();
  }

  /** 처음 맞춰진 뒤 지난 초안을 고르게 한다. */
  function compareEarlier(): void {
    if (checkedEarlier) return;
    checkedEarlier = true;
    showNextOffer();
  }

  /** 지금 내용과 같은 지난 초안은 지우고(이미 반영됨), 남은 것 중 첫째를 고르게 한다. */
  function showNextOffer(): void {
    const all = loadOffers(start.id);
    // 제목과 본문이 모두 지금과 같아야 이미 반영된 것이다(제목만 다른 초안도 고르게 한다).
    const rest = latest === undefined ? all : withoutApplied(all, { title, body: latest });
    for (const applied of all.filter((d) => !rest.includes(d))) removeOffer(start.id, applied);
    // `found`는 $state라 넣은 객체의 프록시가 된다. 어느 것을 고르게 하는지는 따로 적는다(객체 비교 대신).
    foundIsReserved = rest[0] === undefined && reserved !== undefined;
    found = rest[0] ?? reserved;
  }

  /** 지금 고르게 한 초안을 줄에서 뺀다(보존했거나 버렸다). 다음 것이 있으면 이어 고르게 한다. */
  function resolveFound(): void {
    const done = found;
    if (done === undefined) return;
    if (foundIsReserved) {
      foundIsReserved = false;
      clearDraft(start.id);
      reserved = undefined;
    } else {
      removeOffer(start.id, done);
    }
    found = undefined;
    foundRequest = undefined;
    showNextOffer();
  }

  function open(): void {
    const next = openLive(
      start.id,
      {
        name: me.displayName === '' ? m.documents_live_anonymous() : me.displayName,
        color: colorFor(me.userId),
      },
      start.can_edit,
    );
    watch?.stop();
    watch = watchUnsynced(next.document, next.provider, next.persisted, {
      // 편집기가 본문을 넘긴 뒤에 남긴다.
      onDirty: keepDraftSoon,
      onSynced: () => {
        if (keepTimer !== undefined) clearTimeout(keepTimer);
        keepTimer = undefined;
        if (reserved === undefined) clearDraft(start.id);
      },
    });
    next.provider.on('sync', (synced: boolean) => {
      if (synced) setTimeout(compareEarlier, 0);
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
      keepIfDirty();
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
  // 네트워크 오류면 그대로 다시 붙기를 기다린다. 같은 연결인지는 provider로 본다(`session`은 $state라 프록시다).
  let checking = false;
  async function checkRefusal(current: LiveSession): Promise<void> {
    if (checking) return;
    checking = true;
    try {
      const doc = await api.document(start.id);
      if (session?.provider !== current.provider || doc.realtime) return;
      keepIfDirty();
      current.provider.disconnect();
      onrealtimeoff?.();
    } catch (e: unknown) {
      if (session?.provider !== current.provider || !(e instanceof ApiError)) return;
      if (e.status === 401 || e.status === 403 || e.status === 404) {
        keepIfDirty();
        current.provider.disconnect();
        stopped = 'denied';
      }
    } finally {
      checking = false;
    }
  }

  function restart(): void {
    keepIfDirty();
    if (stopped === 'denied' && onreload !== undefined) {
      onreload();
      return;
    }
    if (session !== undefined) closeLive(session);
    session = undefined;
    stopped = undefined;
    // 새 문서로 다시 연다(4409 등). 방금 남긴 초안(서버가 받지 않은 편집)을 줄 뒤에 붙여 다시 맞춰진 뒤 고르게 한다.
    reserved ??= takeOrReserve(start.id).reserved;
    if (found === undefined) checkedEarlier = false;
    epoch += 1;
    open();
  }

  open();
  onDestroy(() => {
    keepIfDirty();
    watch?.stop();
    if (session !== undefined) closeLive(session);
  });

  /** 지난번 초안을 내 노트로 보존한다(실시간 문서에는 섞지 않는다). 다시 눌러도 같은 노트가 된다. */
  async function keepFoundAsNote(): Promise<void> {
    const draft = found;
    if (draft === undefined) return;
    keepingFound = true;
    foundMessage = undefined;
    foundRequest ??= { note: newRequestId(), save: newRequestId() };
    const copyTitle = m.documents_draft_copy_title({
      title: draft.title.trim() === '' ? start.title : draft.title.trim(),
    });
    try {
      const created = await api.createNote(foundRequest.note, copyTitle);
      const saved = await api.saveDocument(created.id, {
        requestId: foundRequest.save,
        baseRevision: created.revision,
        title: copyTitle,
        body: draft.body,
      });
      if (saved.kind === 'conflict') throw new Error('copy conflicted');
      resolveFound();
      router.navigate(standalonePath(created.id));
    } catch (e: unknown) {
      foundMessage = errorText(e, m.documents_copy_failed());
    } finally {
      keepingFound = false;
    }
  }

  function discardFound(): void {
    resolveFound();
  }

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
  {#if found !== undefined}
    <div
      class="flex flex-wrap items-center gap-2 rounded border px-3 py-2 text-sm"
      role="region"
      aria-label={m.documents_draft_region()}
    >
      <span>{m.documents_live_offline_found({ time: formatInstant(found.savedAt) })}</span>
      <Button size="sm" disabled={keepingFound} onclick={() => void keepFoundAsNote()}
        >{m.documents_conflict_keep_copy()}</Button
      >
      <Button size="sm" variant="ghost" disabled={keepingFound} onclick={discardFound}
        >{m.documents_draft_discard()}</Button
      >
    </div>
    {#if foundMessage !== undefined}<p role="alert" class="text-sm text-destructive">
        {foundMessage}
      </p>{/if}
  {/if}
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
        onchange={(body: DocNode) => {
          latest = body;
        }}
      />
    {/key}
  {/if}
  <p class="text-xs text-muted-foreground">
    {m.documents_live_hint()}
  </p>
</section>
