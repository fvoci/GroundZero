<script lang="ts">
  // 충돌 화면의 "내 입력으로 다시 저장"에서 나와 다른 사람이 같은 칸을 서로 다르게 바꿨을 때의 확인(D-94). 칸마다
  // 서버의 지금 값과 내 값을 글로 보이고, 덮어쓸지 묻는다. 취소하면 아무것도 보내지 않는다(입력은 그대로).
  // 값은 줄이지 않고 줄바꿈·공백을 지켜 보인다(길면 칸 안에서 스크롤한다). 서로 다른 값이 같아 보이면 무엇을
  // 덮어쓰는지 알 수 없다.
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

  // Esc는 취소와 같다(아무것도 보내지 않는다). 둘러싼 업무 대화상자는 문서에서 Esc를 받아 닫으므로 여기서 멈춰, 확인만
  // 닫히고 대화상자와 입력은 남게 한다. 처리기는 이 요소에 직접 달아, Svelte의 위임 처리기가 어느 뿌리(앱, 대화상자를
  // 그리는 포털)에 붙든 문서의 대화상자 처리기보다 먼저 받는다.
  let box = $state<HTMLElement | undefined>(undefined);
  $effect(() => {
    const el = box;
    if (el === undefined) return;
    const onkeydown = (e: KeyboardEvent): void => {
      if (e.key !== 'Escape') return;
      e.preventDefault();
      e.stopPropagation();
      if (!busy) oncancel();
    };
    el.addEventListener('keydown', onkeydown);
    return () => {
      el.removeEventListener('keydown', onkeydown);
    };
  });
</script>

<!--
  aria-modal은 두지 않는다: 이 확인은 충돌 알림 안에 놓인 인라인 확인이고, 그 밖의 충돌 알림(서버 내용 불러오기)과
  둘러싼 대화상자의 닫기는 그대로 쓸 수 있다. aria-modal="true"는 화면 읽기 프로그램에 그 바깥을 쓸 수 없다고 알려
  실제로 쓸 수 있는 단추를 숨긴다. 업무 상세의 나머지는 확인이 떠 있는 동안 inert로 실제로 막는다(TaskDetail).
-->
<div
  class="mt-3 flex flex-col gap-2 rounded-md border border-amber-500/50 p-3"
  role="alertdialog"
  aria-labelledby="task-clash-heading"
  aria-describedby="task-clash-hint"
  bind:this={box}
>
  <h3 id="task-clash-heading" class="font-semibold" tabindex="-1" bind:this={heading}>
    {m.tasks_clash_heading()}
  </h3>
  <p id="task-clash-hint" class="text-muted-foreground">{m.tasks_clash_hint()}</p>
  <ul class="flex flex-col gap-3">
    {#each rows as row (row.key)}
      <li data-clash-field={row.key} class="flex flex-col gap-1">
        <span class="font-medium">{row.label}</span>
        <dl class="grid gap-2 sm:grid-cols-2">
          <div class="flex min-w-0 flex-col gap-0.5">
            <dt class="text-muted-foreground">{m.tasks_clash_server()}</dt>
            <dd
              data-clash-side="server"
              class="max-h-60 overflow-auto rounded bg-muted px-2 py-1 break-words whitespace-pre-wrap"
            >
              {row.server}
            </dd>
          </div>
          <div class="flex min-w-0 flex-col gap-0.5">
            <dt class="text-muted-foreground">{m.tasks_clash_mine()}</dt>
            <dd
              data-clash-side="mine"
              class="max-h-60 overflow-auto rounded bg-muted px-2 py-1 break-words whitespace-pre-wrap"
            >
              {row.mine}
            </dd>
          </div>
        </dl>
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
