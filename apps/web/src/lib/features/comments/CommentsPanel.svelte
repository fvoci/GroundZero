<script lang="ts">
  // 댓글(D-27, D-20). 대상을 볼 수 있으면 댓글을 단다. 한 단계 답글, 해결된 스레드는 접어 둔다.
  import type { CommentTarget, CommentView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { RetryKey } from '$lib/api/retry-key';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  import CommentItem from './CommentItem.svelte';
  import { threads } from './threads';

  interface Props {
    target: CommentTarget;
  }

  const { target }: Props = $props();

  let comments = $state<CommentView[]>([]);
  // 이 대상의 목록을 한 번이라도 받았는지. 받기 전에는 "댓글이 없습니다" 대신 불러오는 중이라고 보인다.
  let loaded = $state(false);
  let error = $state<string | undefined>(undefined);
  let status = $state('');
  let draft = $state('');
  let replyTo = $state<string | undefined>(undefined);
  let replyDraft = $state('');
  // 같은 내용의 재시도는 같은 요청 id(응답을 잃고 다시 눌러도 같은 댓글), 고쳐서 보내면 새 id.
  const draftKey = new RetryKey();
  const replyKey = new RetryKey();
  let lastTarget = '';
  let showResolved = $state<Set<string>>(new Set());
  let busy = $state(false);
  let reloadKey = $state(0);
  let seq = 0;

  $effect(() => {
    const t = { kind: target.kind, id: target.id };
    if (reloadKey < 0) return;
    // 대상이 바뀌면 남은 요청 id를 새로 정한다(다른 대상에 옛 id가 쓰이지 않게).
    if (t.id !== lastTarget) {
      lastTarget = t.id;
      draftKey.done();
      replyKey.done();
      // 다른 대상의 댓글을 보이지 않는다. 다시 읽기(작업 뒤)는 받을 때까지 지금 목록을 둔다.
      comments = [];
      loaded = false;
    }
    const mine = ++seq;
    api
      .comments(t)
      .then((list) => {
        if (mine === seq) {
          comments = list;
          loaded = true;
        }
      })
      .catch((e: unknown) => {
        if (mine === seq) error = errorText(e, m.comments_load_failed());
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
      error = errorText(e, m.comments_action_failed());
      return false;
    } finally {
      busy = false;
    }
  }

  async function post(): Promise<void> {
    const body = draft.trim();
    if (body === '') return;
    const requestId = draftKey.for({ target: target.id, body });
    if (await run(() => api.addComment(target, body, null, requestId), m.comments_posted())) {
      draft = '';
      draftKey.done(requestId);
    }
  }

  async function reply(root: string): Promise<void> {
    const body = replyDraft.trim();
    if (body === '') return;
    const requestId = replyKey.for({ target: target.id, root, body });
    if (await run(() => api.addComment(target, body, root, requestId), m.comments_replied())) {
      replyDraft = '';
      replyKey.done(requestId);
      replyTo = undefined;
    }
  }

  let heading = $state<HTMLHeadingElement | undefined>();

  /** 지운 댓글이 사라져도 키보드 초점을 잃지 않게 댓글 제목으로 옮긴다. */
  async function remove(id: string): Promise<void> {
    if (await run(() => api.deleteComment(id), m.comments_deleted())) heading?.focus();
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
  <h2
    id="comments-heading"
    class="text-lg font-semibold focus-visible:outline-none"
    tabindex="-1"
    bind:this={heading}
  >
    {m.comments_heading()}
  </h2>
  <p aria-live="polite" class="text-sm">
    {#if error !== undefined}<span class="text-destructive">{error}</span>{:else}{status}{/if}
  </p>
  <ul class="flex flex-col gap-3" data-testid="comments">
    {#each threads(comments) as t (t.root.id)}
      <li class="flex flex-col gap-2 rounded-md border p-2">
        {#if t.root.resolved && !showResolved.has(t.root.id)}
          <div class="flex items-center gap-2 text-sm text-muted-foreground">
            <span class="min-w-0 flex-1 truncate"
              >{m.comments_resolved_summary({ body: t.root.body })}</span
            >
            <Button
              size="sm"
              variant="ghost"
              onclick={() => (showResolved = new Set([...showResolved, t.root.id]))}
              >{m.comments_expand()}</Button
            >
          </div>
        {:else}
          <CommentItem
            comment={t.root}
            {busy}
            onedit={(body: string) =>
              run(() => api.editComment(t.root.id, body), m.comments_edited())}
            ondelete={() => void remove(t.root.id)}
          />
          {#each t.replies as r (r.id)}
            <div class="ml-4 border-l pl-3">
              <CommentItem
                comment={r}
                {busy}
                onedit={(body: string) =>
                  run(() => api.editComment(r.id, body), m.comments_edited())}
                ondelete={() => void remove(r.id)}
              />
            </div>
          {/each}
          <div class="flex flex-wrap gap-1">
            {#if replyTo !== t.root.id}
              <Button
                size="sm"
                variant="outline"
                onclick={() => {
                  replyTo = t.root.id;
                  replyDraft = '';
                  replyKey.done();
                }}>{m.comments_reply()}</Button
              >
            {/if}
            {#if t.root.can_resolve}
              <Button
                size="sm"
                variant="ghost"
                onclick={() =>
                  void run(
                    () => api.resolveComment(t.root.id, !t.root.resolved),
                    t.root.resolved ? m.comments_unresolved() : m.comments_resolved(),
                  )}>{t.root.resolved ? m.comments_unresolve() : m.comments_resolve()}</Button
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
                aria-label={m.comments_reply()}
                class="min-h-16 rounded-md border bg-background p-2 text-sm"
                bind:value={replyDraft}
                maxlength={10000}
                onkeydown={(e: KeyboardEvent) => {
                  submitOnCtrlEnter(e, () => void reply(t.root.id));
                }}></textarea>
              <div class="flex gap-2">
                <Button type="submit" size="sm" disabled={busy || replyDraft.trim() === ''}
                  >{m.comments_reply_submit()}</Button
                >
                <Button size="sm" variant="ghost" onclick={() => (replyTo = undefined)}
                  >{m.common_cancel()}</Button
                >
              </div>
            </form>
          {/if}
        {/if}
      </li>
    {:else}
      {#if loaded}
        <li class="text-sm text-muted-foreground">{m.comments_empty()}</li>
      {:else if error === undefined}
        <li class="text-sm text-muted-foreground">{m.comments_loading()}</li>
      {/if}
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
      aria-label={m.comments_new_label()}
      class="min-h-20 rounded-md border bg-background p-2 text-sm"
      bind:value={draft}
      maxlength={10000}
      placeholder={m.comments_new_placeholder()}
      onkeydown={(e: KeyboardEvent) => {
        submitOnCtrlEnter(e, () => void post());
      }}></textarea>
    <div>
      <Button type="submit" disabled={busy || draft.trim() === ''}>{m.comments_post()}</Button>
    </div>
  </form>
</section>
