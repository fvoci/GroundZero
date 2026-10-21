<script lang="ts">
  // 충돌 화면의 "내 입력으로 다시 저장"에서 나와 다른 사람이 같은 칸을 서로 다르게 바꿨을 때의 확인(D-94). 칸마다
  // 서버의 지금 값과 내 값을 글로 보이고, 덮어쓸지 묻는다. 취소하면 아무것도 보내지 않는다(입력은 그대로).
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  import type { ClashRow } from './labels';

  interface Props {
    rows: ClashRow[];
    busy: boolean;
    onconfirm: () => void;
    oncancel: () => void;
  }

  const { rows, busy, onconfirm, oncancel }: Props = $props();

  // 뜨면 제목으로 초점을 옮겨 읽히게 한다(키보드로 바로 단추에 닿는다).
  let heading = $state<HTMLElement | undefined>(undefined);
  $effect(() => {
    heading?.focus();
  });
</script>

<div
  class="mt-3 flex flex-col gap-2 rounded-md border border-amber-500/50 p-3"
  role="alertdialog"
  aria-labelledby="task-clash-heading"
  aria-describedby="task-clash-hint"
>
  <h3 id="task-clash-heading" class="font-semibold" tabindex="-1" bind:this={heading}>
    {m.tasks_clash_heading()}
  </h3>
  <p id="task-clash-hint" class="text-muted-foreground">{m.tasks_clash_hint()}</p>
  <ul class="flex flex-col gap-1.5">
    {#each rows as row (row.key)}
      <li data-clash-field={row.key}>
        <span class="font-medium">{row.label}</span>
        <span class="block break-words">{m.tasks_clash_server({ value: row.server })}</span>
        <span class="block break-words">{m.tasks_clash_mine({ value: row.mine })}</span>
      </li>
    {/each}
  </ul>
  <div class="flex flex-wrap gap-2">
    <Button size="sm" disabled={busy} onclick={onconfirm}>{m.tasks_clash_overwrite()}</Button>
    <Button size="sm" variant="outline" disabled={busy} onclick={oncancel}
      >{m.common_cancel()}</Button
    >
  </div>
</div>
