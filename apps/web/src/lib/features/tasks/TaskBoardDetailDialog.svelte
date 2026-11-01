<script lang="ts">
  import type { Workspace } from '@fvoci/api-client';
  import type { TaskView } from './model';
  import TaskDetail from './TaskDetail.svelte';
  import type { CapturedRead } from './context-read';
  import type { DetailRead } from './screen-api';
  import * as Dialog from '$lib/ui/dialog';
  import { m } from '$lib/paraglide/messages.js';
  interface Props {
    aggregate?: boolean;
    takeInitialRead?: ((taskId: string) => CapturedRead<DetailRead> | undefined) | undefined;
    initialReady?: boolean;
    openTaskId: string | undefined;
    itemId: string;
    readOnly: boolean;
    tasks: TaskView[];
    workspaces: Workspace[];
    title: string;
    leaveOpen: boolean;
    returnTo: string | undefined;
    requestClose: () => void;
    requestOpen: (id: string) => void;
    restoreClosedTaskFocus: (id: string) => void;
    onchanged: () => void;
    dialogContent?: HTMLElement | null;
    onunsaved: (value: boolean) => void;
    detailSaving: boolean;
    onsaving: (value: boolean) => void;
    ondraftkept: (value: boolean) => void;
  }
  let {
    aggregate = false,
    takeInitialRead,
    initialReady = false,
    openTaskId,
    itemId,
    readOnly,
    tasks,
    workspaces,
    title,
    leaveOpen,
    returnTo,
    requestClose,
    requestOpen,
    restoreClosedTaskFocus,
    onchanged,
    dialogContent = $bindable(null),
    onunsaved,
    detailSaving,
    onsaving,
    ondraftkept,
  }: Props = $props();
  let detailEditor = $state<{ discardChildInput(): boolean }>();
  export function discardChildInput(): boolean {
    return detailEditor?.discardChildInput() ?? true;
  }
</script>

<!-- 닫기를 대화상자가 정하지 않고 requestClose()가 정한다(고친 입력이 있으면 열어 둔 채 묻는다). -->
<Dialog.Root
  bind:open={
    () => openTaskId !== undefined,
    (o: boolean) => {
      if (!o) requestClose();
    }
  }
>
  <Dialog.Content
    bind:ref={dialogContent}
    class="max-h-[90vh] overflow-y-auto sm:max-w-3xl"
    escapeKeydownBehavior={leaveOpen || detailSaving ? 'ignore' : 'close'}
    interactOutsideBehavior={leaveOpen || detailSaving ? 'ignore' : 'close'}
    showCloseButton={!detailSaving}
    onCloseAutoFocus={(e: Event) => {
      if (returnTo !== undefined) {
        e.preventDefault();
        restoreClosedTaskFocus(returnTo);
      }
    }}
  >
    <Dialog.Header>
      <Dialog.Title>{openTaskId === undefined ? '' : title || m.tasks_task()}</Dialog.Title>
      <Dialog.Description>{m.tasks_dialog_description()}</Dialog.Description>
    </Dialog.Header>
    {#if openTaskId !== undefined && (!aggregate || initialReady)}
      {#key openTaskId}
        <TaskDetail
          {aggregate}
          initialRead={takeInitialRead}
          {readOnly}
          bind:this={detailEditor}
          taskId={openTaskId}
          {itemId}
          {tasks}
          {workspaces}
          {onchanged}
          onopen={requestOpen}
          {onunsaved}
          {ondraftkept}
          {onsaving}
        />
      {/key}
    {/if}
  </Dialog.Content>
</Dialog.Root>
