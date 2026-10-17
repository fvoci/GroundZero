<script lang="ts">
  // 실시간 공동 편집 화면(D-27 워크스페이스별 ON, D-70). 본문은 Yjs로 바로 함께 고치고(저장 단추·초안 없음),
  // 서버가 조용할 때·마지막 사람이 나갈 때 리비전을 남긴다. 제목은 저장 API로 따로 바꾼다.
  import { onDestroy, untrack } from 'svelte';

  import type { DocumentView } from '$lib/api/client';
  import { api, ApiError, fileContentUrl, newRequestId } from '$lib/api/client';
  import { uploadFile } from '$lib/features/files/upload';
  import type { ReferenceContext } from '$lib/features/references/context';
  import { me } from '$lib/me.svelte';
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
      name: me.displayName === '' ? '이름 없음' : me.displayName,
      color: colorFor(me.userId),
    });
    next.provider.on('status', (event: { status: LiveStatus }) => {
      status = event.status;
    });
    next.provider.on('connection-close', (event: CloseEvent | null) => {
      const stop = event === null ? undefined : stopFor(event.code);
      if (stop === undefined) return;
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
    if (done.file_id === null) throw new Error(done.failure ?? '업로드를 마치지 못했습니다');
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
      titleMessage = '제목을 입력하세요.';
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
      titleMessage = '다른 곳에서 동시에 저장했습니다. 다시 시도하세요.';
    } catch (e: unknown) {
      titleMessage = e instanceof ApiError ? e.message : '제목을 저장하지 못했습니다.';
    } finally {
      savingTitle = false;
    }
  }

  const STATUS: Record<LiveStatus, string> = {
    connected: '실시간 연결됨',
    connecting: '연결 중…',
    disconnected: '연결이 끊김 · 다시 연결 중',
  };
  const STOP: Record<LiveStop, string> = {
    denied: '이 문서를 볼 권한이 없어져 연결을 닫았습니다.',
    invalid: '서버가 받지 않는 변경이 있어 연결을 닫았습니다. 새로고침해 주세요.',
    reload: '문서가 바뀌어 다시 엽니다.',
    off: '실시간 편집이 꺼졌습니다.',
  };
</script>

<section class="flex flex-col gap-3" aria-label="실시간 공동 편집">
  <div class="flex flex-wrap items-center gap-2 text-sm">
    <span class="rounded bg-muted px-2 py-0.5 font-mono">{start.reference ?? ''}</span>
    <span role="status" aria-live="polite" class="text-muted-foreground"
      >{stopped === undefined ? STATUS[status] : STOP[stopped]}</span
    >
    {#if stopped !== undefined}
      <Button size="sm" variant="outline" onclick={restart}>다시 연결</Button>
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
      <Label for="live-title">제목</Label>
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
        disabled={savingTitle || title.trim() === savedTitle}>제목 저장</Button
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
        label="본문"
        {references}
        insertFile={start.can_edit ? insertFile : undefined}
        collab={session}
      />
    {/key}
  {/if}
  <p class="text-xs text-muted-foreground">
    여럿이 함께 고치는 문서입니다. 내용은 바로 나누고, 잠시 조용해지거나 모두 나가면 리비전으로
    남습니다.
  </p>
</section>
