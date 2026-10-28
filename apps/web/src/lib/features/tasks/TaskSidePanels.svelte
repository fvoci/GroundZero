<script lang="ts">
  import type { Workspace } from '@fvoci/api-client';
  import type { TaskDetailView } from './model';
  import TaskTimePanel from '$lib/features/time/TaskTimePanel.svelte';
  import CommentsPanel from '$lib/features/comments/CommentsPanel.svelte';
  import Backlinks from '$lib/features/references/Backlinks.svelte';
  const {
    current,
    workspaces,
    onunsaved,
    ondraftkept,
    canWrite,
  }: {
    current: TaskDetailView;
    workspaces: Workspace[];
    canWrite: boolean;
    onunsaved: (value: boolean) => void;
    ondraftkept: (value: boolean) => void;
  } = $props();
  let editor = $state<{ discardInput: () => boolean; isSaving: () => boolean }>();
  export function discardInput(): boolean {
    return editor?.discardInput() ?? true;
  }
  export function isSaving(): boolean {
    return editor?.isSaving() === true;
  }
</script>

<TaskTimePanel taskId={current.id} />
<!-- 댓글 입력은 서버 접근 등급으로 연다(D-113). -->
<CommentsPanel
  bind:this={editor}
  target={{ kind: 'task', id: current.id }}
  {canWrite}
  itemId={current.item_id}
  onunsaved={(value: boolean) => {
    onunsaved(value);
  }}
  ondraftkept={(value: boolean) => {
    ondraftkept(value);
  }}
/>

<Backlinks kind="task" id={current.id} {workspaces} />
