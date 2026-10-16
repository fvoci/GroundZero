<script lang="ts">
  // 업무 상세 (D-31): 필드·담당자·하위 업무·체크리스트·역방향 링크.
  // 필드 저장은 기준 버전으로 충돌을 막는다. 충돌하면 입력을 지우지 않고 서버 내용과 함께 보여 준다(D-53과 같은 원칙).
  import type { Workspace } from '@fvoci/api-client';

  import type {
    AssigneeView,
    ShiftProposalView,
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
    /** 같은 항목의 업무 전체(하위 업무·선행 후보·후속 확인용). */
    tasks: TaskView[];
    workspaces: Workspace[];
    onchanged: () => void;
    onopen: (taskId: string) => void;
  }

  const { taskId, tasks, workspaces, onchanged, onopen }: Props = $props();

  const subtasks = $derived(tasks.filter((t) => t.parent_id === taskId));
  const byId = $derived(new Map(tasks.map((t) => [t.id, t])));
  const successors = $derived(tasks.filter((t) => t.predecessor_ids.includes(taskId)));
  const label = (t: TaskView | undefined): string =>
    t === undefined ? '' : `${t.reference ?? ''} ${t.title}`.trim();

  let task = $state<TaskDetailView | undefined>(undefined);
  let form = $state<TaskInput | undefined>(undefined);
  let tagsText = $state('');
  let estimateText = $state('');
  let people = $state<AssigneeView[]>([]);
  let conflict = $state<TaskDetailView | undefined>(undefined);
  let error = $state<string | undefined>(undefined);
  let saved = $state(false);
  // 날짜를 바꿀 때 함께 옮길 후속 업무(미리보기). 있으면 저장 전에 사용자에게 묻는다(D-31).
  let proposals = $state<ShiftProposalView[] | undefined>(undefined);
  let proposalsRetried = $state(false);
  let predecessorChoice = $state('');
  const candidates = $derived(
    task === undefined
      ? []
      : tasks.filter((t) => t.id !== taskId && !(task?.predecessor_ids.includes(t.id) ?? false)),
  );

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
      milestone: next.milestone,
    };
    proposals = undefined;
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
      start_date: form.milestone || form.start_date === '' ? null : form.start_date,
      due_date: form.due_date === '' ? null : form.due_date,
      estimate_minutes: estimate !== null && Number.isFinite(estimate) ? estimate : null,
      tags: tagsText
        .split(',')
        .map((t) => t.trim())
        .filter((t) => t !== ''),
    };
  }

  /** 저장: 날짜가 바뀌고 후속 업무가 있으면 먼저 영향을 미리 보고, 옮길 것이 있으면 사용자에게 묻는다. */
  async function submit(baseVersion: number): Promise<void> {
    const input = currentInput();
    if (task === undefined || input === undefined) return;
    const datesChanged = input.start_date !== task.start_date || input.due_date !== task.due_date;
    const hasSuccessors = tasks.some((t) => t.predecessor_ids.includes(taskId));
    if (datesChanged && hasSuccessors) {
      error = undefined;
      try {
        const found = await api.previewSchedule(task.id, input.start_date, input.due_date);
        if (found.length > 0) {
          proposals = found;
          proposalsRetried = false;
          return;
        }
      } catch (e: unknown) {
        error = e instanceof ApiError ? e.message : '후속 업무 영향을 계산하지 못했습니다.';
        return;
      }
    }
    await save(baseVersion, []);
  }

  async function save(baseVersion: number, shifts: ShiftProposalView[]): Promise<void> {
    const input = currentInput();
    if (task === undefined || input === undefined) return;
    error = undefined;
    saved = false;
    try {
      const result = await api.updateTask(
        task.id,
        baseVersion,
        input,
        shifts.map((s) => ({
          task_id: s.task_id,
          base_version: s.base_version,
          start_date: s.start_date,
          due_date: s.due_date,
        })),
      );
      if (result.kind === 'conflict') {
        conflict = result.current;
        return;
      }
      if (result.kind === 'shift_conflict') {
        // 아무것도 바뀌지 않았다. 지금 상태로 다시 계산해 다시 묻는다.
        proposals = await api.previewSchedule(task.id, input.start_date, input.due_date);
        proposalsRetried = true;
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

  async function changePredecessor(action: Promise<TaskDetailView>): Promise<void> {
    error = undefined;
    try {
      const fresh = await action;
      if (task?.version === fresh.version) task = fresh;
      else fill(fresh);
      onchanged();
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '선행 업무를 바꾸지 못했습니다.';
    }
  }

  const range = (start: string | null, due: string | null): string =>
    start !== null && due !== null && start !== due
      ? `${start}~${due}`
      : (due ?? start ?? '날짜 없음');

  let shiftHeading = $state<HTMLElement | undefined>(undefined);
  $effect(() => {
    if (proposals !== undefined) shiftHeading?.focus();
  });

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
          <Button size="sm" onclick={() => void submit(latest.version)}
            >내 입력으로 다시 저장</Button
          >
        </div>
      </div>
    {/if}

    <form
      class="grid gap-3 md:grid-cols-2"
      onsubmit={(e) => {
        e.preventDefault();
        void submit(current.version);
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
        <Input
          id="task-start"
          type="date"
          bind:value={f.start_date}
          disabled={!canEdit || f.milestone}
        />
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
      <div class="flex items-center gap-2 md:col-span-2">
        <Checkbox
          id="task-milestone"
          checked={f.milestone}
          disabled={!canEdit}
          onCheckedChange={(v) => {
            f.milestone = v === true;
            if (f.milestone) f.start_date = null;
          }}
        />
        <Label for="task-milestone">마일스톤 (마감일 하나만)</Label>
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

    {#if proposals !== undefined}
      {@const shifts = proposals}
      <section
        class="flex flex-col gap-2 rounded-md border border-amber-500/50 p-3 text-sm"
        aria-labelledby="shift-heading"
      >
        <h3 id="shift-heading" class="font-semibold" tabindex="-1" bind:this={shiftHeading}>
          후속 업무 {shifts.length}개와 날짜가 겹칩니다
        </h3>
        {#if proposalsRetried}
          <p role="alert">후속 업무가 그사이 바뀌어 다시 계산했습니다.</p>
        {/if}
        <p class="text-muted-foreground">
          선행 업무가 끝난 다음 날 시작하도록 옮기자고 제안합니다. 각 업무의 기간은 그대로입니다.
        </p>
        <ul class="flex flex-col gap-1">
          {#each shifts as s (s.task_id)}
            <li>
              <span class="font-mono text-xs text-muted-foreground">{s.reference ?? ''}</span>
              {s.title}: {range(s.from_start_date, s.from_due_date)} → {range(
                s.start_date,
                s.due_date,
              )}
            </li>
          {/each}
        </ul>
        <div class="flex flex-wrap gap-2">
          <Button size="sm" onclick={() => void save(current.version, shifts)}
            >후속 업무도 옮겨 저장</Button
          >
          <Button size="sm" variant="outline" onclick={() => void save(current.version, [])}
            >이 업무만 저장</Button
          >
          <Button size="sm" variant="ghost" onclick={() => (proposals = undefined)}>취소</Button>
        </div>
      </section>
    {/if}

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

    <section class="flex flex-col gap-2" aria-labelledby="predecessors-heading">
      <h3 id="predecessors-heading" class="text-sm font-semibold">선행 업무 (끝나야 시작)</h3>
      {#each current.predecessor_ids as pid (pid)}
        <div class="flex items-center gap-2 text-sm">
          <button
            type="button"
            class="rounded px-2 py-1 text-left hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            onclick={() => {
              onopen(pid);
            }}>{label(byId.get(pid)) || '볼 수 없는 업무'}</button
          >
          {#if canEdit}
            <Button
              size="sm"
              variant="ghost"
              class="ml-auto"
              aria-label={`${label(byId.get(pid))} 선행 관계 지우기`}
              onclick={() => void changePredecessor(api.removePredecessor(current.id, pid))}
              >지우기</Button
            >
          {/if}
        </div>
      {:else}
        <p class="text-sm text-muted-foreground">없습니다.</p>
      {/each}
      {#if successors.length > 0}
        <p class="text-sm text-muted-foreground">
          이 업무 뒤에 오는 업무: {successors.map((t) => label(t)).join(', ')}
        </p>
      {/if}
      {#if canEdit && candidates.length > 0}
        <div class="flex flex-wrap items-center gap-2">
          <span id="predecessor-label" class="text-sm font-medium">선행 업무 추가</span>
          <Select.Root
            type="single"
            value={predecessorChoice}
            onValueChange={(v: string) => (predecessorChoice = v)}
          >
            <Select.Trigger aria-labelledby="predecessor-label"
              >{predecessorChoice === ''
                ? '업무 고르기'
                : label(byId.get(predecessorChoice))}</Select.Trigger
            >
            <Select.Content>
              {#each candidates as c (c.id)}
                <Select.Item value={c.id} label={label(c)}>{label(c)}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
          <Button
            size="sm"
            disabled={predecessorChoice === ''}
            onclick={() => {
              const chosen = predecessorChoice;
              predecessorChoice = '';
              void changePredecessor(api.addPredecessor(current.id, chosen));
            }}>추가</Button
          >
        </div>
      {/if}
    </section>

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
