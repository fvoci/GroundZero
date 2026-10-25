<script lang="ts">
  // 이름 하나를 받아 만드는 작은 폼. 같은 입력의 재시도는 같은 request_id를 쓴다.
  import { NetworkError, newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  interface Props {
    id: string;
    label: string;
    placeholder: string;
    submitLabel: string;
    create: (requestId: string, name: string) => Promise<void>;
    /** 열자마자 입력란에 초점(누른 단추가 이 폼으로 바뀌는 곳). */
    autofocus?: boolean;
    /** 있으면 Esc로 닫는다(초점을 돌려줄 곳은 부르는 쪽이 정한다). */
    oncancel?: () => void;
  }

  const {
    id,
    label,
    placeholder,
    submitLabel,
    create,
    autofocus = false,
    oncancel,
  }: Props = $props();

  let input = $state<HTMLInputElement | null>(null);
  $effect(() => {
    if (autofocus) input?.focus();
  });

  let name = $state('');
  let busy = $state(false);
  let error = $state<string | undefined>(undefined);
  let pending: { requestId: string; name: string } | undefined;

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const trimmed = name.trim();
    if (trimmed === '') {
      error = m.workspaces_name_required({ label });
      return;
    }
    if (pending?.name !== trimmed) pending = { requestId: newRequestId(), name: trimmed };
    busy = true;
    error = undefined;
    try {
      await create(pending.requestId, trimmed);
      pending = undefined;
      name = '';
    } catch (e: unknown) {
      // 연결 실패는 다시 눌러도 같은 request_id라 중복되지 않는다는 점을 알린다.
      error =
        e instanceof NetworkError
          ? m.workspaces_create_network_failed()
          : errorText(e, m.workspaces_create_failed());
    } finally {
      busy = false;
    }
  }
</script>

<form class="flex flex-wrap items-end gap-2" onsubmit={(e) => void submit(e)} novalidate>
  <div class="flex min-w-0 flex-1 basis-40 flex-col gap-1">
    <Label for={id}>{label}</Label>
    <Input
      bind:ref={input}
      {id}
      bind:value={name}
      onkeydown={(e: KeyboardEvent) => {
        if (e.key === 'Escape' && oncancel !== undefined && !busy) {
          e.preventDefault();
          oncancel();
        }
      }}
      {placeholder}
      maxlength={200}
      autocomplete="off"
      aria-invalid={error !== undefined}
      aria-describedby={error !== undefined ? `${id}-error` : undefined}
    />
  </div>
  <Button type="submit" disabled={busy}>{submitLabel}</Button>
</form>
{#if error !== undefined}
  <p id={`${id}-error`} class="text-sm text-destructive" role="alert">{error}</p>
{/if}
