<script lang="ts">
  // 댓글(D-27, D-20). 서버가 쓰기를 허용한 대상에만 댓글을 단다. 한 단계 답글, 해결된 스레드는 접어 둔다.
  import type { CommentTarget, CommentView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { accountSwitching, notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { errorText, isArchivedRejection } from '$lib/api/errors';
  import { me } from '$lib/me.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  import CommentComposer from './CommentComposer.svelte';
  import CommentItem from './CommentItem.svelte';
  import { threads } from './threads';

  interface Props {
    target: CommentTarget;
    /** 부모가 받은 서버 접근 등급으로만 정한다. 생략하면 입력을 열지 않는다. */
    canWrite?: boolean;
    archived?: boolean;
    /** 문서·업무 응답에는 보관 칸이 없어, 안내용 부모 상태를 읽을 때 쓴다. */
    itemId?: string | null;
    onunsaved?: (value: boolean) => void;
    ondraftkept?: (value: boolean) => void;
  }

  const { target, canWrite = false, archived, itemId, onunsaved, ondraftkept }: Props = $props();
  let parentArchived = $state(false);
  const isArchived = $derived(archived ?? parentArchived);
  const writable = $derived(canWrite && !isArchived);

  $effect(() => {
    const id = itemId;
    parentArchived = false;
    if (canWrite || archived !== undefined || id === undefined || id === null) return;
    let cancelled = false;
    const active = (): boolean => !cancelled;
    void api
      .item(id)
      .then(async (item) => {
        if (!active()) return;
        parentArchived = item.archived;
        const module = await api.module(item.module_id);
        if (active()) parentArchived = item.archived || module.archived;
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  });

  let comments = $state<CommentView[]>([]);
  // 이 대상의 목록을 한 번이라도 받았는지. 받기 전에는 "댓글이 없습니다" 대신 불러오는 중이라고 보인다.
  let loaded = $state(false);
  let error = $state<string | undefined>(undefined);
  let unsaved = $state<Record<string, boolean>>({});
  let kept = $state<Record<string, boolean>>({});
  $effect(() => {
    onunsaved?.(Object.values(unsaved).some(Boolean));
    ondraftkept?.(Object.entries(unsaved).every(([key, dirty]) => !dirty || kept[key] === true));
  });
  let lastTarget = '';
  let showResolved = $state<Set<string>>(new Set());
  let busy = $state(false);
  let editors = $state<Record<string, { discardInput(): void } | undefined>>({});

  export function isSaving(): boolean {
    return busy;
  }

  export function discardInput(): boolean {
    if (busy) return false;
    for (const editor of Object.values(editors)) editor?.discardInput();
    return true;
  }
  let reloadKey = $state(0);
  let seq = 0;

  $effect(() => {
    const t = { kind: target.kind, id: target.id };
    const identity = `${me.userId}:${t.kind}:${t.id}`;
    if (reloadKey < 0) return;
    // 대상이 바뀌면 남은 요청 id를 새로 정한다(다른 대상에 옛 id가 쓰이지 않게).
    if (identity !== lastTarget) {
      lastTarget = identity;
      unsaved = {};
      kept = {};
      busy = false;
      // 다른 대상의 댓글·오류·완료 문구를 보이지 않는다. 다시 읽기(작업 뒤)는 받을 때까지 지금 목록을 둔다.
      comments = [];
      loaded = false;
      error = undefined;
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
    if (busy || accountSwitching()) return false;
    const identity = `${me.userId}:${target.kind}:${target.id}`;
    const current = (): boolean =>
      identity === `${me.userId}:${target.kind}:${target.id}` && !accountSwitching();
    busy = true;
    error = undefined;
    try {
      await action();
      if (!current()) return false;
      notifyDone(done);
      reloadKey += 1;
      return true;
    } catch (e: unknown) {
      if (!current()) return false;
      error = isArchivedRejection(e)
        ? m.common_error_archived()
        : errorText(e, m.comments_action_failed());
      return false;
    } finally {
      if (current()) busy = false;
    }
  }

  function post(body: string, root: string | null, requestId: string): Promise<boolean> {
    if (!writable) return Promise.resolve(false);
    return run(
      () => api.addComment(target, body, root, requestId),
      root === null ? m.comments_posted() : m.comments_replied(),
    );
  }

  let heading = $state<HTMLHeadingElement | undefined>();

  /** 지운 댓글이 사라져도 키보드 초점을 잃지 않게 댓글 제목으로 옮긴다. */
  async function remove(id: string): Promise<void> {
    if (await run(() => api.deleteComment(id), m.comments_deleted())) heading?.focus();
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
  <!-- 목록이 비었을 때의 불러오는 중·빈 목록 글도 이 알림 영역에서 바뀌어 화면 낭독기가 끝났음을 읽는다(리뷰). -->
  <p role="status" aria-live="polite" class="text-sm">
    {#if error !== undefined}<span class="text-destructive">{error}</span>{/if}
    {#if isArchived}<span>{m.comments_archived()}</span>{/if}
    {#if comments.length === 0 && error === undefined}
      <span class="text-muted-foreground">{loaded ? m.comments_empty() : m.comments_loading()}</span
      >
    {/if}
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
            bind:this={editors[`edit:${t.root.id}`]}
            comment={t.root}
            canWrite={writable}
            {busy}
            onedit={(body: string) =>
              run(() => api.editComment(t.root.id, body), m.comments_edited())}
            ondelete={() => void remove(t.root.id)}
            onunsaved={(value: boolean) => {
              unsaved[`edit:${t.root.id}`] = value;
            }}
            ondraftkept={(value: boolean) => {
              kept[`edit:${t.root.id}`] = value;
            }}
          />
          {#each t.replies as r (r.id)}
            <div class="ml-4 border-l pl-3">
              <CommentItem
                bind:this={editors[`edit:${r.id}`]}
                comment={r}
                canWrite={writable}
                {busy}
                onedit={(body: string) =>
                  run(() => api.editComment(r.id, body), m.comments_edited())}
                ondelete={() => void remove(r.id)}
                onunsaved={(value: boolean) => {
                  unsaved[`edit:${r.id}`] = value;
                }}
                ondraftkept={(value: boolean) => {
                  kept[`edit:${r.id}`] = value;
                }}
              />
            </div>
          {/each}
          <div class="flex flex-wrap gap-1">
            {#if writable && t.root.can_resolve}
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
          <CommentComposer
            bind:this={editors[`reply:${t.root.id}`]}
            {target}
            root={t.root.id}
            {writable}
            {busy}
            onpost={post}
            onunsaved={(value: boolean) => {
              unsaved[`reply:${t.root.id}`] = value;
            }}
            ondraftkept={(value: boolean) => {
              kept[`reply:${t.root.id}`] = value;
            }}
          />
        {/if}
      </li>
    {/each}
  </ul>
  <CommentComposer
    bind:this={editors.new}
    {target}
    {writable}
    {busy}
    onpost={post}
    onunsaved={(value: boolean) => {
      unsaved.new = value;
    }}
    ondraftkept={(value: boolean) => {
      kept.new = value;
    }}
  />
</section>
