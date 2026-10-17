<script lang="ts">
  // 댓글(D-27, D-20). 대상을 볼 수 있으면 댓글을 단다. 한 단계 답글, 해결된 스레드는 접어 둔다.
  import type { CommentTarget, CommentView } from '$lib/api/client';
  import { api, ApiError, newRequestId } from '$lib/api/client';
  import { Button } from '$lib/ui/button';

  import CommentItem from './CommentItem.svelte';
  import { threads } from './threads';

  interface Props {
    target: CommentTarget;
  }

  const { target }: Props = $props();

  let comments = $state<CommentView[]>([]);
  let error = $state<string | undefined>(undefined);
  let status = $state('');
  let draft = $state('');
  let replyTo = $state<string | undefined>(undefined);
  let replyDraft = $state('');
  let showResolved = $state<Set<string>>(new Set());
  let busy = $state(false);
  let reloadKey = $state(0);
  let seq = 0;

  $effect(() => {
    const t = { kind: target.kind, id: target.id };
    if (reloadKey < 0) return;
    const mine = ++seq;
    api
      .comments(t)
      .then((list) => {
        if (mine === seq) comments = list;
      })
      .catch((e: unknown) => {
        if (mine === seq) error = e instanceof ApiError ? e.message : '댓글을 불러오지 못했습니다.';
      });
  });

  async function run(action: () => Promise<unknown>, done: string): Promise<boolean> {
    if (busy) return false;
    busy = true;
    error = undefined;
    try {
      await action();
      status = done;
      reloadKey += 1;
      return true;
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '처리하지 못했습니다.';
      return false;
    } finally {
      busy = false;
    }
  }

  async function post(): Promise<void> {
    const body = draft.trim();
    if (body === '') return;
    if (await run(() => api.addComment(target, body, null, newRequestId()), '댓글을 달았습니다.'))
      draft = '';
  }

  async function reply(root: string): Promise<void> {
    const body = replyDraft.trim();
    if (body === '') return;
    if (await run(() => api.addComment(target, body, root, newRequestId()), '답글을 달았습니다.')) {
      replyDraft = '';
      replyTo = undefined;
    }
  }

  /** 답글 칸이 열리면 바로 쓸 수 있게 초점을 옮긴다(키보드 사용). */
  function focusOnMount(node: HTMLTextAreaElement): void {
    node.focus();
  }

  function submitOnCtrlEnter(e: KeyboardEvent, send: () => void): void {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      send();
    }
  }
</script>

<section class="flex flex-col gap-3" aria-labelledby="comments-heading">
  <h2 id="comments-heading" class="text-lg font-semibold">댓글</h2>
  <p aria-live="polite" class="text-sm">
    {#if error !== undefined}<span class="text-destructive">{error}</span>{:else}{status}{/if}
  </p>
  <ul class="flex flex-col gap-3" data-testid="comments">
    {#each threads(comments) as t (t.root.id)}
      <li class="flex flex-col gap-2 rounded-md border p-2">
        {#if t.root.resolved && !showResolved.has(t.root.id)}
          <div class="flex items-center gap-2 text-sm text-muted-foreground">
            <span class="min-w-0 flex-1 truncate">해결됨 · {t.root.body}</span>
            <Button
              size="sm"
              variant="ghost"
              onclick={() => (showResolved = new Set([...showResolved, t.root.id]))}>펼치기</Button
            >
          </div>
        {:else}
          <CommentItem
            comment={t.root}
            {busy}
            onedit={(body: string) =>
              run(() => api.editComment(t.root.id, body), '댓글을 고쳤습니다.')}
            ondelete={() => void run(() => api.deleteComment(t.root.id), '댓글을 지웠습니다.')}
          />
          {#each t.replies as r (r.id)}
            <div class="ml-4 border-l pl-3">
              <CommentItem
                comment={r}
                {busy}
                onedit={(body: string) =>
                  run(() => api.editComment(r.id, body), '댓글을 고쳤습니다.')}
                ondelete={() => void run(() => api.deleteComment(r.id), '댓글을 지웠습니다.')}
              />
            </div>
          {/each}
          <div class="flex flex-wrap gap-1">
            {#if replyTo !== t.root.id}
              <Button
                size="sm"
                variant="outline"
                onclick={() => ((replyTo = t.root.id), (replyDraft = ''))}>답글</Button
              >
            {/if}
            {#if t.root.can_resolve}
              <Button
                size="sm"
                variant="ghost"
                onclick={() =>
                  void run(
                    () => api.resolveComment(t.root.id, !t.root.resolved),
                    t.root.resolved ? '해결을 풀었습니다.' : '해결로 표시했습니다.',
                  )}>{t.root.resolved ? '해결 풀기' : '해결'}</Button
              >
            {/if}
          </div>
          {#if replyTo === t.root.id}
            <form
              class="ml-4 flex flex-col gap-2"
              onsubmit={(e: SubmitEvent) => {
                e.preventDefault();
                void reply(t.root.id);
              }}
            >
              <textarea
                use:focusOnMount
                aria-label="답글"
                class="min-h-16 rounded-md border bg-background p-2 text-sm"
                bind:value={replyDraft}
                maxlength={10000}
                onkeydown={(e: KeyboardEvent) => {
                  submitOnCtrlEnter(e, () => void reply(t.root.id));
                }}></textarea>
              <div class="flex gap-2">
                <Button type="submit" size="sm" disabled={busy || replyDraft.trim() === ''}
                  >답글 달기</Button
                >
                <Button size="sm" variant="ghost" onclick={() => (replyTo = undefined)}>취소</Button
                >
              </div>
            </form>
          {/if}
        {/if}
      </li>
    {:else}
      <li class="text-sm text-muted-foreground">댓글이 없습니다.</li>
    {/each}
  </ul>
  <form
    class="flex flex-col gap-2"
    onsubmit={(e: SubmitEvent) => {
      e.preventDefault();
      void post();
    }}
  >
    <textarea
      aria-label="새 댓글(Ctrl·⌘+Enter로 달기)"
      class="min-h-20 rounded-md border bg-background p-2 text-sm"
      bind:value={draft}
      maxlength={10000}
      placeholder="댓글"
      onkeydown={(e: KeyboardEvent) => {
        submitOnCtrlEnter(e, () => void post());
      }}></textarea>
    <div><Button type="submit" disabled={busy || draft.trim() === ''}>댓글 달기</Button></div>
  </form>
</section>
