<script lang="ts">
  import { accountSwitching } from '$lib/features/feedback/feedback.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';

  import { timer } from './timer.svelte';

  let continueButton = $state<HTMLButtonElement | null>(null);
  $effect(() => {
    if (accountSwitching()) timer.chooseRestart(undefined);
  });
</script>

<Dialog.Root
  open={timer.restartPending && !accountSwitching()}
  onOpenChange={(open) => {
    if (!open) timer.chooseRestart(undefined);
  }}
>
  <Dialog.Content
    onOpenAutoFocus={(event: Event) => {
      event.preventDefault();
      continueButton?.focus();
    }}
  >
    <Dialog.Header>
      <Dialog.Title>{m.time_restart_title()}</Dialog.Title>
      <Dialog.Description>{m.time_restart_description()}</Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer>
      <Button
        variant="ghost"
        onclick={() => {
          timer.chooseRestart(undefined);
        }}>{m.common_cancel()}</Button
      >
      <Button
        variant="outline"
        onclick={() => {
          timer.chooseRestart('new');
        }}>{m.time_restart_new()}</Button
      >
      <Button
        bind:ref={continueButton}
        onclick={() => {
          timer.chooseRestart('continue');
        }}>{m.time_restart_continue()}</Button
      >
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
