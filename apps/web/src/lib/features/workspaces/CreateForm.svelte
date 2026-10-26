<script lang="ts">
  // 이름 하나를 받아 만드는 작은 폼. 같은 입력의 재시도는 같은 request_id를 쓴다. 부르는 쪽이 `onchange`로 입력을 브라우저에
  // 남길 수 있고(D-96·D-128 업무 만들기), 되살릴 때는 `initial`로 이름과 그때의 request_id를 시드한다(같은 이름이면 같은
  // request_id라 응답을 잃은 만들기를 다시 눌러도 중복되지 않는다).
  import { onMount, untrack } from 'svelte';

  import { NetworkError, newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import { me } from '$lib/me.svelte';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import type { CreateFormContext } from '$lib/features/common/form-draft';

  /** 아직 만들지 않은 입력(이름과, 그 이름으로 보냈던 request_id). */
  interface CreateDraft {
    name: string;
    requestId: string | null;
  }

  interface Props {
    id: string;
    label: string;
    placeholder: string;
    submitLabel: string;
    create: (requestId: string, name: string, context: CreateFormContext) => Promise<void>;
    /** Stable form kind + container/parent ID. Omit for externally managed drafts. */
    draftName?: string;
    /** Related selection (for example a template ID), preserved with the name. */
    draftContext?: string;
    /** 열자마자 입력란에 초점(누른 단추가 이 폼으로 바뀌는 곳). */
    autofocus?: boolean;
    /** 있으면 Esc로 닫는다(초점을 돌려줄 곳은 부르는 쪽이 정한다). */
    oncancel?: () => void;
    /** 되살린 입력으로 시작한다(처음 그릴 때 한 번 읽는다). */
    initial?: CreateDraft | undefined;
    /** 입력이 바뀔 때마다(비었거나 만들었거나 Esc로 버렸으면 undefined) 알린다. */
    onchange?: ((draft: CreateDraft | undefined) => void) | undefined;
  }

  let {
    id,
    label,
    placeholder,
    submitLabel,
    create,
    autofocus = false,
    oncancel,
    initial,
    onchange,
    draftName,
    draftContext = $bindable(''),
  }: Props = $props();

  let input = $state<HTMLInputElement | null>(null);
  $effect(() => {
    if (autofocus) input?.focus();
  });

  // 되살린 시작값은 처음 한 번만 읽는다.
  const start = untrack(() => initial);
  let name = $state(start?.name ?? '');
  let busy = $state(false);
  let error = $state<string | undefined>(undefined);
  let pending: { requestId: string; name: string; context: string } | undefined =
    start !== undefined && start.requestId !== null
      ? { requestId: start.requestId, name: start.name.trim(), context: '' }
      : undefined;

  const draft = createFormDraft({
    name: () => draftName,
    snapshot: () => ({
      name,
      requestId:
        pending?.name === name.trim() && pending.context === draftContext
          ? pending.requestId
          : null,
      context: draftContext,
    }),
    parse: (raw) => {
      if (
        typeof raw !== 'object' ||
        raw === null ||
        !('name' in raw) ||
        typeof raw.name !== 'string' ||
        !('requestId' in raw) ||
        (raw.requestId !== null && typeof raw.requestId !== 'string') ||
        !('context' in raw) ||
        typeof raw.context !== 'string'
      )
        return undefined;
      return { name: raw.name, requestId: raw.requestId, context: raw.context };
    },
    restore: (value) => {
      name = value.name;
      pending =
        value.requestId === null
          ? undefined
          : { requestId: value.requestId, name: value.name.trim(), context: value.context };
      draftContext = value.context;
    },
    reset: () => {
      name = '';
      pending = undefined;
      draftContext = '';
      error = undefined;
      busy = false;
    },
    dirty: () => name !== '' || draftContext !== '',
    busy: () => busy,
  });
  onMount(draft.attach);

  function report(typed: string): void {
    const trimmed = typed.trim();
    onchange?.(
      trimmed === ''
        ? undefined
        : { name: typed, requestId: pending?.name === trimmed ? pending.requestId : null },
    );
  }
  $effect(() => {
    // 친 글자(되살린 시작값 포함)를 알린다.
    const typed = name;
    untrack(() => {
      report(typed);
    });
  });

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (busy || (draftName !== undefined && (!draft.active || draft.blocked))) return;
    const trimmed = name.trim();
    if (trimmed === '') {
      error = m.workspaces_name_required({ label });
      return;
    }
    if (pending?.name !== trimmed || pending.context !== draftContext)
      pending = { requestId: newRequestId(), name: trimmed, context: draftContext };
    report(name);
    busy = true;
    error = undefined;
    const attempt = draft.capture();
    const managed = draftName !== undefined;
    const userId = me.userId;
    let completed = false;
    const saved = (): boolean => {
      if (completed) return true;
      if (managed && (attempt === undefined || !draft.complete(attempt))) return false;
      completed = true;
      pending = undefined;
      name = '';
      onchange?.(undefined);
      return true;
    };
    try {
      await create(pending.requestId, trimmed, { userId, saved });
      saved();
    } catch (e: unknown) {
      if (managed && (attempt === undefined || !draft.isCurrent(attempt))) return;
      // 연결 실패는 다시 눌러도 같은 request_id라 중복되지 않는다는 점을 알린다.
      error =
        e instanceof NetworkError
          ? m.workspaces_create_network_failed()
          : errorText(e, m.workspaces_create_failed());
    } finally {
      if (!managed || (attempt !== undefined && draft.isOwner(attempt))) busy = false;
    }
  }
</script>

{#each draft.offers as entry (entry.key)}
  <FormDraftOffer
    onrestore={() => {
      draft.restore(entry);
    }}
    ondiscard={() => {
      draft.discard(entry);
    }}
  />
{/each}
<DiscardChangesConfirm
  open={draft.leave.open}
  onkeep={draft.leave.keep}
  ondiscard={draft.discardAndLeave}
/>

<form class="flex flex-wrap items-end gap-2" onsubmit={(e) => void submit(e)} novalidate>
  <div class="flex min-w-0 flex-1 basis-40 flex-col gap-1">
    <Label for={id}>{label}</Label>
    <Input
      bind:ref={input}
      {id}
      bind:value={name}
      disabled={busy || (draftName !== undefined && !draft.active)}
      onkeydown={(e: KeyboardEvent) => {
        if (e.key === 'Escape' && oncancel !== undefined && !busy) {
          e.preventDefault();
          const cancel = () => {
            name = '';
            pending = undefined;
            draftContext = '';
            draft.persist();
            onchange?.(undefined);
            oncancel();
          };
          if (draftName === undefined) cancel();
          else draft.leave.request(cancel);
        }
      }}
      {placeholder}
      maxlength={200}
      autocomplete="off"
      aria-invalid={error !== undefined}
      aria-describedby={error !== undefined ? `${id}-error` : undefined}
    />
  </div>
  <Button
    type="submit"
    disabled={busy || (draftName !== undefined && (!draft.active || draft.blocked))}
    >{submitLabel}</Button
  >
</form>
{#if error !== undefined}
  <p id={`${id}-error`} class="text-sm text-destructive" role="alert">{error}</p>
{/if}
