<script lang="ts">
  // 저장하지 않은 입력이 있는 업무·일정 대화상자를 닫으려 할 때의 확인. "계속 편집"(Esc와 같음)은 이 확인만 닫고 초점을
  // 닫으려 하기 전 자리로 돌린다(입력은 그대로). "변경 버리고 닫기"를 고를 때만 입력을 버린다(대화상자는 부르는 쪽이
  // 닫는다). 입력이 그대로면 부르는 쪽이 묻지 않고 바로 닫는다. 대화상자 안에서 다른 업무를 열 때도 같은 확인을 쓴다
  // (문구만 바꾼다).
  import { m } from '$lib/paraglide/messages.js';
  import * as AlertDialog from '$lib/ui/alert-dialog';

  interface Props {
    open: boolean;
    /** 확인만 닫는다(계속 편집·Esc). */
    onkeep: () => void;
    /** 입력을 버리고 대화상자를 닫는다. */
    ondiscard: () => void;
    /** 계속 편집하면 초점을 돌려줄 요소. 없으면(null) 확인을 열 때 초점이 있던 요소로 돌아간다. */
    returnFocus?: (() => HTMLElement | null) | undefined;
    /** 닫지 않고 다른 것을 열 때의 설명·버리기 단추 글(없으면 닫기 문구). */
    description?: string | undefined;
    confirmLabel?: string | undefined;
  }

  const { open, onkeep, ondiscard, returnFocus, description, confirmLabel }: Props = $props();

  // 버리기를 골랐는지. 그러면 닫히는 대화상자 안으로 초점을 돌리지 않는다(대화상자가 닫으며 초점을 정한다).
  let discarded = false;
  $effect(() => {
    if (open) discarded = false;
  });
</script>

<AlertDialog.Root
  bind:open={
    () => open,
    (o: boolean) => {
      if (!o && !discarded) onkeep();
    }
  }
>
  <AlertDialog.Content
    onCloseAutoFocus={(e: Event) => {
      if (discarded) {
        e.preventDefault();
        return;
      }
      const target = returnFocus?.() ?? null;
      if (target !== null) {
        e.preventDefault();
        target.focus();
      }
    }}
  >
    <AlertDialog.Header>
      <AlertDialog.Title>{m.common_discard_title()}</AlertDialog.Title>
      <AlertDialog.Description
        >{description ?? m.common_discard_description()}</AlertDialog.Description
      >
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel>{m.common_discard_keep()}</AlertDialog.Cancel>
      <AlertDialog.Action
        variant="destructive"
        onclick={() => {
          discarded = true;
          ondiscard();
        }}>{confirmLabel ?? m.common_discard_confirm()}</AlertDialog.Action
      >
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
