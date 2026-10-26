<script lang="ts">
  // 댓글 하나: 쓴 사람·시각, 본문, 고치기(쓴 사람)·지우기(쓴 사람·소유자).
  import { formatInstant } from '$lib/display';
  import { onDestroy } from 'svelte';
  import type { CommentView } from '$lib/api/client';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import { accountSwitching } from '$lib/features/feedback/feedback.svelte';
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
    onunsaved?: (value: boolean) => void;
  }

  const { comment, busy, canWrite = true, onedit, ondelete, onunsaved }: Props = $props();

  let editing = $state<string | undefined>(undefined);
  let original = $state('');
  let confirming = $state(false);
  const draft = createFormDraft({
    name: () => `comment-edit:${comment.id}`,
    snapshot: () => ({ text: editing ?? '', original }),
    parse: (value: unknown) => {
      if (
        typeof value !== 'object' ||
        value === null ||
        !('text' in value) ||
        typeof value.text !== 'string' ||
        !('original' in value) ||
        typeof value.original !== 'string'
      )
        return undefined;
      return { text: value.text, original: value.original };
    },
    restore: (value) => {
      editing = value.text;
      original = value.original;
    },
    reset: () => {
      editing = undefined;
      original = '';
    },
    dirty: () => editing !== undefined && editing !== original,
    busy: () => busy,
  });
  $effect(() => {
    onunsaved?.(editing !== undefined && editing !== original && !accountSwitching());
  });
  onDestroy(() => {
    onunsaved?.(false);
  });

  async function save(): Promise<void> {
    if (
      editing === undefined ||
      busy ||
      !canWrite ||
      !comment.can_edit ||
      accountSwitching() ||
      draft.blocked
    )
      return;
    const attempt = draft.capture();
    if (attempt === undefined) return;
    const value = editing;
    if (await onedit(value.trim())) {
      draft.complete(attempt);
    }
  }

  function cancel(): void {
    if (!draft.active) return;
    draft.leave.request(() => {
      editing = undefined;
      draft.persist();
    });
  }
</script>

<DiscardChangesConfirm
  open={draft.active && draft.leave.open}
  description={m.comments_discard_edit()}
  onkeep={draft.leave.keep}
  ondiscard={draft.leave.discard}
/>

<div class="flex flex-col gap-1">
  <div class="flex flex-wrap items-baseline gap-2 text-sm">
    <span class="font-medium">{comment.author_name ?? m.comments_unknown_author()}</span>
    <span class="text-xs text-muted-foreground"
      >{formatInstant(comment.created_at)}{comment.edited
        ? ` · ${m.comments_edited_mark()}`
        : ''}</span
    >
  </div>
  {#each draft.offers as offer (offer.key)}
    <div data-testid="comment-edit-draft-offer">
      <p class="text-sm whitespace-pre-wrap">{offer.value.input.text}</p>
      <FormDraftOffer
        onrestore={() => {
          draft.restore(offer);
        }}
        ondiscard={() => {
          draft.discard(offer);
        }}
      />
    </div>
  {/each}
  {#if canWrite && comment.can_edit && editing !== undefined && !accountSwitching()}
    <form
      class="flex flex-col gap-2"
      onsubmit={(e: SubmitEvent) => {
        e.preventDefault();
        void save();
      }}
    >
      <Textarea aria-label={m.comments_edit_label()} bind:value={editing} maxlength={10000} />
      <div class="flex gap-2">
        <Button type="submit" size="sm" disabled={busy || draft.blocked}>{m.common_save()}</Button>
        <Button size="sm" variant="ghost" onclick={cancel}>{m.common_cancel()}</Button>
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
      <Button
        size="sm"
        variant="ghost"
        onclick={() => {
          original = comment.body;
          editing = comment.body;
        }}>{m.comments_edit()}</Button
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
