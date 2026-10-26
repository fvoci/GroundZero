<script lang="ts">
  import { onDestroy } from 'svelte';

  import type { CommentTarget } from '$lib/api/client';
  import { newRequestId } from '$lib/api/client';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import { accountSwitching } from '$lib/features/feedback/feedback.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Textarea } from '$lib/ui/textarea';

  interface Props {
    target: CommentTarget;
    root?: string;
    writable: boolean;
    busy: boolean;
    onpost: (body: string, root: string | null, requestId: string) => Promise<boolean>;
    onunsaved: (value: boolean) => void;
  }
  const { target, root, writable, busy, onpost, onunsaved }: Props = $props();
  let text = $state('');
  let request = $state<{ body: string; requestId: string } | null>(null);
  let open = $state(false);

  const draft = createFormDraft({
    name: () => `comment-compose:${target.kind}:${target.id}:${root ?? 'new'}`,
    snapshot: () => ({ text, request }),
    parse: (value: unknown) => {
      if (
        typeof value !== 'object' ||
        value === null ||
        !('text' in value) ||
        typeof value.text !== 'string' ||
        !('request' in value)
      )
        return undefined;
      const r = value.request;
      if (r === null) return { text: value.text, request: null };
      if (
        typeof r !== 'object' ||
        !('body' in r) ||
        typeof r.body !== 'string' ||
        !('requestId' in r) ||
        typeof r.requestId !== 'string'
      )
        return undefined;
      return { text: value.text, request: { body: r.body, requestId: r.requestId } };
    },
    restore: (value) => {
      text = value.text;
      request = value.request;
      open = true;
    },
    reset: () => {
      text = '';
      request = null;
      open = false;
    },
    dirty: () => text !== '',
    busy: () => busy,
  });

  $effect(() => {
    onunsaved(text !== '' && !accountSwitching());
  });
  onDestroy(() => {
    onunsaved(false);
  });

  async function post(): Promise<void> {
    if (!writable || busy || accountSwitching() || draft.blocked) return;
    const body = text.trim();
    if (body === '') return;
    if (request?.body !== body) request = { body, requestId: newRequestId() };
    const attempt = draft.capture();
    if (attempt === undefined) return;
    if (await onpost(body, root ?? null, request.requestId)) {
      draft.complete(attempt);
    }
  }

  function cancel(): void {
    if (!draft.active) return;
    draft.leave.request(() => {
      text = '';
      request = null;
      open = false;
      draft.persist();
    });
  }
</script>

<DiscardChangesConfirm
  open={draft.active && draft.leave.open}
  description={m.comments_discard_reply()}
  onkeep={draft.leave.keep}
  ondiscard={draft.leave.discard}
/>

{#each draft.offers as offer (offer.key)}
  <div data-testid={root === undefined ? 'comment-draft-offer' : 'comment-reply-draft-offer'}>
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
{#if writable && !accountSwitching()}
  {#if root !== undefined && !open}
    <Button size="sm" variant="outline" onclick={() => (open = true)}>{m.comments_reply()}</Button>
  {:else}
    <form
      class="flex flex-col gap-2"
      onsubmit={(event) => {
        event.preventDefault();
        void post();
      }}
    >
      <Textarea
        {@attach (node: HTMLTextAreaElement) => {
          if (root !== undefined) node.focus();
        }}
        aria-label={root === undefined ? m.comments_new_label() : m.comments_reply()}
        class={root === undefined ? 'min-h-20' : ''}
        bind:value={text}
        maxlength={10000}
        placeholder={root === undefined ? m.comments_new_placeholder() : undefined}
        onkeydown={(event: KeyboardEvent) => {
          if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
            event.preventDefault();
            void post();
          }
        }}
      />
      <div class="flex gap-2">
        <Button
          type="submit"
          size={root === undefined ? 'default' : 'sm'}
          disabled={busy || text.trim() === '' || draft.blocked}
        >
          {root === undefined ? m.comments_post() : m.comments_reply_submit()}
        </Button>
        {#if root !== undefined}<Button size="sm" variant="ghost" onclick={cancel}
            >{m.common_cancel()}</Button
          >{/if}
      </div>
    </form>
  {/if}
{/if}
