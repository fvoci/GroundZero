<script lang="ts">
  // 댓글 하나: 쓴 사람·시각, 본문, 고치기(쓴 사람)·지우기(쓴 사람·소유자).
  import { formatInstant } from '$lib/display';
  import type { CommentView } from '$lib/api/client';
  import { m } from '$lib/paraglide/messages.js';
  import { focusConfirm, focusConfirmStart } from '$lib/features/common/inline-confirm';
  import { Button } from '$lib/ui/button';
  import { Textarea } from '$lib/ui/textarea';

  interface Props {
    comment: CommentView;
    busy: boolean;
    canWrite?: boolean;
    onedit: (body: string) => Promise<boolean>;
    ondelete: () => void;
  }

  const { comment, busy, canWrite = true, onedit, ondelete }: Props = $props();

  let editing = $state<string | undefined>(undefined);
  let confirming = $state(false);
</script>

<div class="flex flex-col gap-1">
  <div class="flex flex-wrap items-baseline gap-2 text-sm">
    <span class="font-medium">{comment.author_name ?? m.comments_unknown_author()}</span>
    <span class="text-xs text-muted-foreground"
      >{formatInstant(comment.created_at)}{comment.edited
        ? ` · ${m.comments_edited_mark()}`
        : ''}</span
    >
  </div>
  {#if canWrite && comment.can_edit && editing !== undefined}
    <form
      class="flex flex-col gap-2"
      onsubmit={async (e: SubmitEvent) => {
        e.preventDefault();
        if (editing !== undefined && (await onedit(editing.trim()))) editing = undefined;
      }}
    >
      <Textarea aria-label={m.comments_edit_label()} bind:value={editing} maxlength={10000} />
      <div class="flex gap-2">
        <Button type="submit" size="sm" disabled={busy}>{m.common_save()}</Button>
        <Button size="sm" variant="ghost" onclick={() => (editing = undefined)}
          >{m.common_cancel()}</Button
        >
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
    {#if canWrite && comment.can_edit && editing === undefined}
      <Button size="sm" variant="ghost" onclick={() => (editing = comment.body)}
        >{m.comments_edit()}</Button
      >
    {/if}
    {#if comment.can_delete && !confirming}
      <Button
        size="sm"
        variant="ghost"
        data-confirm={`comment-${comment.id}`}
        onclick={() => {
          confirming = true;
          void focusConfirm(`comment-${comment.id}`);
        }}>{m.comments_delete()}</Button
      >
    {:else if comment.can_delete}
      <span
        class="flex items-center gap-1 text-sm"
        role="group"
        aria-label={m.comments_delete_confirm_label()}
      >
        {m.comments_delete_confirm()}
        <Button
          size="sm"
          variant="destructive"
          data-confirm-go={`comment-${comment.id}`}
          disabled={busy}
          onclick={() => {
            confirming = false;
            ondelete();
          }}>{m.comments_delete()}</Button
        >
        <Button
          size="sm"
          variant="ghost"
          onclick={() => {
            confirming = false;
            void focusConfirmStart(`comment-${comment.id}`);
          }}>{m.common_cancel()}</Button
        >
      </span>
    {/if}
  </div>
</div>
