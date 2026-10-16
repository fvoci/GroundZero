<script lang="ts">
  // 업무 상세 (D-31): 필드·담당자·하위 업무·체크리스트·역방향 링크.
  // 필드 저장은 기준 버전으로 충돌을 막는다. 충돌하면 입력을 지우지 않고 서버 내용과 함께 보여 준다(D-53과 같은 원칙).
  import type { Workspace } from '@fvoci/api-client';

  import type {
    AssigneeView,
    TaskDetailView,
    TaskInput,
    TaskPriority,
    TaskStatus,
    TaskView,
  } from '$lib/api/client';
  import { api, ApiError, TASK_PRIORITIES, TASK_STATUSES } from '$lib/api/client';
  import type { NewChecklistEntry } from '$lib/features/checklists/checklist-item';
  import ChecklistForm from '$lib/features/checklists/ChecklistForm.svelte';
  import Backlinks from '$lib/features/references/Backlinks.svelte';
  import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import * as Select from '$lib/ui/select';

  import { PRIORITY_LABEL, STATUS_LABEL } from './labels';

  interface Props {
    taskId: string;
    subtasks: TaskView[];
    workspaces: Workspace[];
    onchanged: () => void;
    onopen: (taskId: string) => void;
  }

  const { taskId, subtasks, workspaces, onchanged, onopen }: Props = $props();

  let task = $state<TaskDetailView | undefined>(undefined);
  let form = $state<TaskInput | undefined>(undefined);
  let tagsText = $state('');
  let estimateText = $state('');
  let people = $state<AssigneeView[]>([]);
  let conflict = $state<TaskDetailView | undefined>(undefined);
  let error = $state<string | undefined>(undefined);
  let saved = $state(false);

  function fill(next: TaskDetailView): void {
    task = next;
    form = {
      title: next.title,
      description: next.description,
      status: next.status,
      priority: next.priority,
      start_date: next.start_date,
      due_date: next.due_date,
      estimate_minutes: next.estimate_minutes,
      tags: next.tags,
      assignee_ids: next.assignees.map((a) => a.user_id),
    };
    tagsText = next.tags.join(', ');
    estimateText = next.estimate_minutes === null ? '' : String(next.estimate_minutes);
  }

  $effect(() => {
    const id = taskId;
    let cancelled = false;
    task = undefined;
    conflict = undefined;
    error = undefined;
    void api
      .task(id)
      .then(async (detail) => {
        if (cancelled) return;
        fill(detail);
        people = await api.assignable(detail.item_id);
      })
      .catch((e: unknown) => {
        if (!cancelled)
          error =
            e instanceof ApiError && e.status === 404
              ? '업무가 없거나 볼 권한이 없습니다.'
              : '업무를 불러오지 못했습니다.';
      });
    return () => {
      cancelled = true;
    };
  });

  const canEdit = $derived(task !== undefined && task.access !== 'read');

  function currentInput(): TaskInput | undefined {
    if (form === undefined) return undefined;
    const estimate = estimateText.trim() === '' ? null : Number.parseInt(estimateText, 10);
    return {
      ...form,
      start_date: form.start_date === '' ? null : form.start_date,
      due_date: form.due_date === '' ? null : form.due_date,
      estimate_minutes: estimate !== null && Number.isFinite(estimate) ? estimate : null,
      tags: tagsText
        .split(',')
        .map((t) => t.trim())
        .filter((t) => t !== ''),
    };
  }

  async function save(baseVersion: number): Promise<void> {
    const input = currentInput();
    if (task === undefined || input === undefined) return;
    error = undefined;
    saved = false;
    try {
      const result = await api.updateTask(task.id, baseVersion, input);
      if (result.kind === 'conflict') {
        conflict = result.current;
        return;
      }
      conflict = undefined;
      fill(result.task);
      saved = true;
      onchanged();
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '저장하지 못했습니다.';
    }
  }

  async function reloadChecklist(next: Promise<unknown>): Promise<void> {
    await next;
    if (task === undefined) return;
    const fresh = await api.task(task.id);
    if (task.version === fresh.version) {
      task = fresh;
    } else {
      fill(fresh);
    }
    onchanged();
  }

  function toggleAssignee(id: string, on: boolean): void {
    if (form === undefined) return;
    form.assignee_ids = on
      ? [...new Set([...form.assignee_ids, id])]
      : form.assignee_ids.filter((x) => x !== id);
  }
</script>

{#if error !== undefined}
  <p role="alert" class="text-sm text-destructive">{error}</p>
{/if}
{#if task !== undefined && form !== undefined}
  {@const current = task}
  {@const f = form}
  <div class="flex flex-col gap-4">
    <div class="flex flex-wrap items-baseline gap-2">
      <span class="font-mono text-sm text-muted-foreground">{current.reference ?? ''}</span>
      {#if current.access === 'read'}<span class="text-sm text-muted-foreground">읽기 전용</span
        >{/if}
    </div>

    {#if conflict !== undefined}
      {@const latest = conflict}
      <div class="rounded-md border border-amber-500/50 p-3 text-sm" role="alert">
        <p>
          그사이 다른 사람이 이 업무를 바꿨습니다(지금 제목: “{latest.title}”, 상태: {STATUS_LABEL[
            latest.status
          ]}). 내 입력은 그대로 있습니다.
        </p>
        <div class="mt-2 flex gap-2">
          <Button
            size="sm"
            variant="outline"
            onclick={() => {
              fill(latest);
              conflict = undefined;
            }}>서버 내용 불러오기</Button
          >
          <Button size="sm" onclick={() => void save(latest.version)}>내 입력으로 다시 저장</Button>
        </div>
      </div>
    {/if}

    <form
      class="grid gap-3 md:grid-cols-2"
      onsubmit={(e) => {
        e.preventDefault();
        void save(current.version);
      }}
    >
      <div class="flex flex-col gap-1 md:col-span-2">
        <Label for="task-title">제목</Label>
        <Input id="task-title" bind:value={f.title} disabled={!canEdit} maxlength={200} />
      </div>
      <div class="flex flex-col gap-1 md:col-span-2">
        <Label for="task-description">설명</Label>
        <textarea
          id="task-description"
          bind:value={f.description}
          disabled={!canEdit}
          rows={3}
          class="rounded-md border border-input bg-transparent px-3 py-2 text-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        ></textarea>
      </div>
      <div class="flex flex-col gap-1">
        <span class="text-sm font-medium" id="task-status-label">상태</span>
        <Select.Root
          type="single"
          value={f.status}
          disabled={!canEdit}
          onValueChange={(v: string) => {
            const s = TASK_STATUSES.find((x) => x === v);
            if (s !== undefined) f.status = s satisfies TaskStatus;
          }}
        >
          <Select.Trigger aria-labelledby="task-status-label"
            >{STATUS_LABEL[f.status]}</Select.Trigger
          >
          <Select.Content>
            {#each TASK_STATUSES as s (s)}
              <Select.Item value={s} label={STATUS_LABEL[s]}>{STATUS_LABEL[s]}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
      <div class="flex flex-col gap-1">
        <span class="text-sm font-medium" id="task-priority-label">우선순위</span>
        <Select.Root
          type="single"
          value={f.priority}
          disabled={!canEdit}
          onValueChange={(v: string) => {
            const p = TASK_PRIORITIES.find((x) => x === v);
            if (p !== undefined) f.priority = p satisfies TaskPriority;
          }}
        >
          <Select.Trigger aria-labelledby="task-priority-label"
            >{PRIORITY_LABEL[f.priority]}</Select.Trigger
          >
          <Select.Content>
            {#each TASK_PRIORITIES as p (p)}
              <Select.Item value={p} label={PRIORITY_LABEL[p]}>{PRIORITY_LABEL[p]}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
      <div class="flex flex-col gap-1">
        <Label for="task-start">시작일</Label>
        <Input id="task-start" type="date" bind:value={f.start_date} disabled={!canEdit} />
      </div>
      <div class="flex flex-col gap-1">
        <Label for="task-due">마감일</Label>
        <Input id="task-due" type="date" bind:value={f.due_date} disabled={!canEdit} />
      </div>
      <div class="flex flex-col gap-1">
        <Label for="task-estimate">예상 시간(분)</Label>
        <Input
          id="task-estimate"
          inputmode="numeric"
          bind:value={estimateText}
          disabled={!canEdit}
        />
      </div>
      <div class="flex flex-col gap-1">
        <Label for="task-tags">태그(쉼표로 구분)</Label>
        <Input id="task-tags" bind:value={tagsText} disabled={!canEdit} />
      </div>
      <fieldset class="flex flex-col gap-2 md:col-span-2">
        <legend class="text-sm font-medium">담당자 (같은 완료 상태를 공유)</legend>
        <div class="flex flex-wrap gap-3">
          {#each people as person (person.user_id)}
            <span class="flex items-center gap-1.5">
              <Checkbox
                id={`assignee-${person.user_id}`}
                checked={f.assignee_ids.includes(person.user_id)}
                disabled={!canEdit}
                onCheckedChange={(v) => {
                  toggleAssignee(person.user_id, v === true);
                }}
              />
              <Label for={`assignee-${person.user_id}`}>{person.display_name}</Label>
            </span>
          {:else}
            <span class="text-sm text-muted-foreground">지정할 수 있는 사람이 없습니다.</span>
          {/each}
        </div>
      </fieldset>
      {#if canEdit}
        <div class="flex items-center gap-2 md:col-span-2">
          <Button type="submit">저장</Button>
          {#if saved}<span class="text-sm" role="status">저장했습니다.</span>{/if}
        </div>
      {/if}
    </form>

    {#if current.parent_id === null}
      <section class="flex flex-col gap-2" aria-labelledby="subtasks-heading">
        <h3 id="subtasks-heading" class="text-sm font-semibold">하위 업무</h3>
        {#each subtasks as sub (sub.id)}
          <button
            type="button"
            class="flex items-baseline gap-2 rounded px-2 py-1 text-left text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            onclick={() => {
              onopen(sub.id);
            }}
          >
            <span class="font-mono text-xs text-muted-foreground">{sub.reference ?? ''}</span>
            <span>{sub.title}</span>
            <span class="ml-auto text-xs text-muted-foreground">{STATUS_LABEL[sub.status]}</span>
          </button>
        {:else}
          <p class="text-sm text-muted-foreground">없습니다.</p>
        {/each}
        {#if canEdit}
          <CreateForm
            id="new-subtask"
            label="하위 업무 제목"
            placeholder="예: 입력 검증"
            submitLabel="하위 업무 추가"
            create={async (requestId: string, title: string) => {
              await api.createTask(current.item_id, requestId, title, current.id);
              onchanged();
            }}
          />
        {/if}
      </section>
    {/if}

    <ChecklistForm
      idPrefix={`task-${current.id}`}
      items={current.checklist.map((c) => ({ id: c.id, text: c.text, done: c.checked }))}
      {canEdit}
      onadd={(entries: NewChecklistEntry[]) =>
        reloadChecklist(
          api.addChecklist(
            current.id,
            entries.map((e) => ({ request_id: e.requestId, text: e.text, checked: e.done })),
          ),
        )}
      ontoggle={(id: string, done: boolean) => {
        const entry = current.checklist.find((c) => c.id === id);
        return reloadChecklist(api.updateChecklistItem(id, entry?.text ?? '', done));
      }}
      onremove={(id: string) => reloadChecklist(api.deleteChecklistItem(id))}
    />

    <Backlinks kind="task" id={current.id} {workspaces} />
  </div>
{/if}
