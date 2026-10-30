<script lang="ts">
  import type { TaskView } from './model';
  import TaskTimeStart from '$lib/features/time/TaskTimeStart.svelte';
  import { m } from '$lib/paraglide/messages.js';
  const { list, open }: { list: TaskView[]; open: (id: string) => void } = $props();
</script>

<!-- 여섯 칸 표는 좁은 화면에서 옆으로 넘치므로 표만 가로로 스크롤한다(기록 표와 같다). -->
<div class="overflow-x-auto">
  <table class="w-full text-sm" data-testid="task-list">
    <thead class="text-left text-muted-foreground">
      <tr
        ><th class="py-1 font-medium">{m.tasks_col_number()}</th><th class="font-medium"
          >{m.tasks_title()}</th
        ><th class="font-medium">{m.tasks_status()}</th><th class="font-medium"
          >{m.tasks_assignee()}</th
        ><th class="font-medium">{m.tasks_col_due()}</th><th class="font-medium"
          >{m.tasks_col_checklist()}</th
        ></tr
      >
    </thead>
    <tbody>
      {#each list as t (t.id)}
        <tr class="border-t">
          <td class="py-1 font-mono text-xs text-muted-foreground">{t.reference ?? ''}</td>
          <td class={t.parent_id === null ? '' : 'pl-4'}>
            <button
              type="button"
              data-task-card={t.id}
              class="text-left hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
              onclick={() => {
                open(t.id);
              }}>{t.parent_id === null ? '' : '↳ '}{t.title}</button
            >
            <!-- 시간 기록 시작은 업무 행 안에 둔다(D-117). -->
            <TaskTimeStart taskId={t.id} title={t.title} />
          </td>
          <td>{t.status}</td>
          <td>{t.assignees.map((a) => a.display_name).join(', ')}</td>
          <td>{t.due_date ?? ''}</td>
          <td
            >{t.checklist_total > 0
              ? `${String(t.checklist_done)}/${String(t.checklist_total)}`
              : ''}</td
          >
        </tr>
      {:else}
        <tr><td colspan="6" class="py-2 text-muted-foreground">{m.tasks_empty()}</td></tr>
      {/each}
    </tbody>
  </table>
</div>
