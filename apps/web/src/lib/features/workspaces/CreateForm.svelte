<script lang="ts">
  // 이름 하나를 받아 만드는 작은 폼. 같은 입력의 재시도는 같은 request_id를 쓴다.
  import { ApiError, NetworkError, newRequestId } from '$lib/api/client';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  interface Props {
    id: string;
    label: string;
    placeholder: string;
    submitLabel: string;
    create: (requestId: string, name: string) => Promise<void>;
  }

  const { id, label, placeholder, submitLabel, create }: Props = $props();

  let name = $state('');
  let busy = $state(false);
  let error = $state<string | undefined>(undefined);
  let pending: { requestId: string; name: string } | undefined;

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const trimmed = name.trim();
    if (trimmed === '') {
      error = `${label}을(를) 입력하세요.`;
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
      if (e instanceof NetworkError)
        error = '서버에 연결할 수 없습니다. 다시 시도하면 중복 없이 처리됩니다.';
      else if (e instanceof ApiError) error = e.message;
      else error = '만들지 못했습니다.';
    } finally {
      busy = false;
    }
  }
</script>

<form class="flex items-end gap-2" onsubmit={(e) => void submit(e)} novalidate>
  <div class="flex flex-1 flex-col gap-1">
    <Label for={id}>{label}</Label>
    <Input
      {id}
      bind:value={name}
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
