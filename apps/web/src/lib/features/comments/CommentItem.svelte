<script lang="ts">
  // 댓글 하나: 쓴 사람·시각, 본문, 고치기(쓴 사람)·지우기(쓴 사람·소유자).
  import type { CommentView } from '$lib/api/client';
  import { Button } from '$lib/ui/button';

  interface Props {
    comment: CommentView;
    busy: boolean;
    onedit: (body: string) => Promise<boolean>;
    ondelete: () => void;
  }

  const { comment, busy, onedit, ondelete }: Props = $props();

  let editing = $state<string | undefined>(undefined);
</script>

<div class="flex flex-col gap-1">
  <div class="flex flex-wrap items-baseline gap-2 text-sm">
    <span class="font-medium">{comment.author_name ?? '공유받은 사람'}</span>
    <span class="text-xs text-muted-foreground"
      >{new Date(comment.created_at).toLocaleString()}{comment.edited ? ' · 고침' : ''}</span
    >
  </div>
  {#if editing !== undefined}
    <form
      class="flex flex-col gap-2"
      onsubmit={async (e: SubmitEvent) => {
        e.preventDefault();
        if (editing !== undefined && (await onedit(editing.trim()))) editing = undefined;
      }}
    >
      <textarea
        aria-label="댓글 고치기"
        class="min-h-16 rounded-md border bg-background p-2 text-sm"
        bind:value={editing}
        maxlength={10000}></textarea>
      <div class="flex gap-2">
        <Button type="submit" size="sm" disabled={busy}>저장</Button>
        <Button size="sm" variant="ghost" onclick={() => (editing = undefined)}>취소</Button>
      </div>
    </form>
  {:else}
    <p
      class={['text-sm whitespace-pre-wrap', comment.deleted ? 'text-muted-foreground italic' : '']}
    >
      {comment.body}
    </p>
  {/if}
  <div class="flex flex-wrap gap-1">
    {#if comment.can_edit && editing === undefined}
      <Button size="sm" variant="ghost" onclick={() => (editing = comment.body)}>고치기</Button>
    {/if}
    {#if comment.can_delete}
      <Button size="sm" variant="ghost" onclick={ondelete}>지우기</Button>
    {/if}
  </div>
</div>
