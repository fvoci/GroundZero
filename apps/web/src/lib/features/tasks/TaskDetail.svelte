<script lang="ts">
  // 업무 상세 (D-31): 필드·담당자·하위 업무·체크리스트·역방향 링크.
  // 필드 저장은 기준 버전으로 충돌을 막는다. 충돌하면 입력을 지우지 않고 서버 내용과 함께 보여 준다(D-53과 같은 원칙).
  import { tick } from 'svelte';

  import type { Workspace } from '@fvoci/api-client';

  import type {
    AssigneeView,
    LinkBase,
    LinkKind,
    RecurrenceRule,
    ShiftProposalView,
    TaskDetailView,
    TaskInput,
    TaskPriority,
    TaskStatus,
    TaskView,
    PlanBlockedView,
    UndatedTaskView,
  } from '$lib/api/client';
  import {
    api,
    ApiError,
    LINK_KINDS,
    MAX_LAG_DAYS,
    TASK_PRIORITIES,
    TASK_STATUSES,
  } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import type { NewChecklistEntry } from '$lib/features/checklists/checklist-item';
  import { canonicalJson } from '$lib/features/documents/drafts';
  import ChecklistForm from '$lib/features/checklists/ChecklistForm.svelte';
  import RecurrenceEditor from '$lib/features/events/RecurrenceEditor.svelte';
  import TaskTimePanel from '$lib/features/time/TaskTimePanel.svelte';
  import CommentsPanel from '$lib/features/comments/CommentsPanel.svelte';
  import MarkButtons from '$lib/features/home/MarkButtons.svelte';
  import DeleteButton from '$lib/features/trash/DeleteButton.svelte';
  import { itemPath, router } from '$lib/router.svelte';
  import { localTimeZone } from '$lib/features/events/time';
  import Backlinks from '$lib/features/references/Backlinks.svelte';
  import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import * as Select from '$lib/ui/select';

  import { estimateOf, formOf, mergeEstimate, mergeForm } from './form';
  import {
    LINK_KIND_LABEL,
    LINK_KIND_MEANING,
    PRIORITY_LABEL,
    STATUS_LABEL,
    linkText,
    undatedText,
  } from './labels';
  import ShiftProposals from './ShiftProposals.svelte';

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
  // 후속 업무는 같은 관계 목록을 거꾸로 찾는다(관계 정본은 하나, D-91).
  const successors = $derived(
    tasks.flatMap((t) => {
      const link = t.predecessors.find((l) => l.task_id === taskId);
      return link === undefined ? [] : [{ task: t, link }];
    }),
  );
  const label = (t: TaskView | undefined): string =>
    t === undefined ? '' : `${t.reference ?? ''} ${t.title}`.trim();

  let task = $state<TaskDetailView | undefined>(undefined);
  let form = $state<TaskInput | undefined>(undefined);
  let tagsText = $state('');
  let estimateText = $state('');
  // 지정할 수 있는 사람. 받기 전(undefined)에는 "없습니다"를 보이지 않는다.
  let people = $state<AssigneeView[] | undefined>(undefined);
  // 지정할 수 있는 사람을 받지 못했을 때의 안내(업무는 받았으므로 업무 불러오기 실패와 따로 알린다).
  let peopleError = $state<string | undefined>(undefined);
  let conflict = $state<TaskDetailView | undefined>(undefined);
  let error = $state<string | undefined>(undefined);
  let saved = $state(false);
  // 날짜를 바꿀 때 함께 옮길 후속 업무(미리보기). 패널이 떠 있는 동안 폼은 잠그고, 승인하면 본 입력과 기준 버전
  // 그대로 보낸다(D-31: 보여 준 영향을 사용자가 승인).
  interface Pending {
    proposals: ShiftProposalView[];
    undated: UndatedTaskView[];
    blocked: PlanBlockedView | null;
    input: TaskInput;
    baseVersion: number;
    retried: boolean;
  }
  let pending = $state<Pending | undefined>(undefined);
  // 저장했지만 날짜가 없어 옮기지 않은 후속 업무 알림(날짜를 지어내지 않는다).
  let undatedNotice = $state('');
  let busy = $state(false);
  let predecessorChoice = $state('');
  // 관계 고치기(D-91): 고르는 선행 업무(새 관계) 또는 고치는 관계, 종류, 지연일(글자 그대로 두고 저장할 때 확인).
  let editing = $state<string | undefined>(undefined);
  let linkKind = $state<LinkKind>('FS');
  let lagText = $state('0');
  // 편집을 시작한 관계의 종류·지연일(새 관계면 null). 저장할 때 함께 보내 그사이 다른 곳에서 바뀌었으면 서버가 409로
  // 거절한다(말없이 덮지 않음).
  let linkBase = $state<LinkBase | null>(null);
  let linkStatus = $state('');
  // 관계를 바꾸면 옮겨야 하는 후속 업무(미리보기). 승인하면 본 관계·목록 그대로 보낸다.
  interface LinkPending {
    predecessor: string;
    kind: LinkKind;
    lag: number;
    base: LinkBase | null;
    proposals: ShiftProposalView[];
    undated: UndatedTaskView[];
    blocked: PlanBlockedView | null;
    retried: boolean;
  }
  let linkPending = $state<LinkPending | undefined>(undefined);
  // 상위 업무 고르기. 'top'은 최상위로 빼기. 서버 규칙(한 단계, 하위 업무가 있으면 옮길 수 없음)에 맞는 후보만 보인다.
  const TOP = 'top';
  let parentChoice = $state('');
  let parentStatus = $state('');
  const parentCandidates = $derived(
    tasks.filter((t) => t.id !== taskId && t.parent_id === null && t.id !== task?.parent_id),
  );
  const candidates = $derived(
    task === undefined
      ? []
      : tasks.filter((t) => t.id !== taskId && !(task?.predecessor_ids.includes(t.id) ?? false)),
  );

  // 다시 채울 때 비교할 기준(마지막으로 다 불러온 판의 값, 사용자가 바꾸지 않은 칸만 새 값으로 옮긴다)과, 같은 칸을
  // 양쪽에서 바꿔 충돌이 있었는지(다음 저장·불러오기까지 지킨다), 그때 남겨 둘 옛 판 번호.
  let baseline: TaskInput | undefined;
  let baseVersion = 0;
  let clashed = false;

  function fill(next: TaskDetailView): void {
    task = next;
    form = formOf(next);
    baseline = formOf(next);
    baseVersion = next.version;
    clashed = false;
    pending = undefined;
    tagsText = next.tags.join(', ');
    estimateText = next.estimate_minutes === null ? '' : String(next.estimate_minutes);
  }

  /**
   * 체크리스트·반복·선행 업무를 바꾼 뒤 서버의 새 판으로 다시 채운다. 고쳤지만 저장하지 않은 칸(태그·예상 시간은
   * 저장할 때 보내는 값으로 비교)은 지키고, 그 칸을 서버에서도 누가 바꿨으면 옛 판 번호를 남겨 저장할 때 충돌
   * 화면이 고르게 한다.
   */
  function refresh(fresh: TaskDetailView): void {
    const mine = currentInput();
    if (task === undefined || mine === undefined || baseline === undefined) {
      fill(fresh);
      return;
    }
    const freshForm = formOf(fresh);
    const before = baseline;
    // 예상 시간은 입력한 글자의 값으로 따로 맞춘다(병합에는 고치지 않은 칸으로 넘긴다).
    const estimate = mergeEstimate(
      before.estimate_minutes,
      estimateText,
      freshForm.estimate_minutes,
    );
    const merged = mergeForm(
      before,
      { ...mine, estimate_minutes: before.estimate_minutes },
      freshForm,
    );
    const tagsTouched = canonicalJson(mine.tags) !== canonicalJson(before.tags);
    baseline = { ...merged.baseline, estimate_minutes: estimate.baseline };
    // 충돌은 지금의 병합으로 다시 본다. 기준은 사용자가 바꾼 칸을 옛 값으로 지키므로, 서버가 그 칸을 다르게 두는 한 계속
    // 충돌이고, 사용자가 서버 값으로 맞추면 풀려 최신 판으로 저장한다.
    clashed = merged.clash || estimate.clash;
    if (!clashed) baseVersion = fresh.version;
    task = clashed ? { ...fresh, version: baseVersion } : fresh;
    form = merged.form;
    pending = undefined;
    if (!tagsTouched) tagsText = freshForm.tags.join(', ');
    if (!estimate.keepText) {
      estimateText = freshForm.estimate_minutes === null ? '' : String(freshForm.estimate_minutes);
    }
  }

  $effect(() => {
    const id = taskId;
    let cancelled = false;
    task = undefined;
    conflict = undefined;
    error = undefined;
    void api
      .task(id)
      .then((detail) => {
        if (cancelled) return;
        fill(detail);
        peopleError = undefined;
        void api.assignable(detail.item_id).then(
          (assignable) => {
            if (!cancelled) people = assignable;
          },
          (e: unknown) => {
            if (cancelled) return;
            people = undefined;
            peopleError = errorText(e, m.tasks_assignable_load_failed());
          },
        );
      })
      .catch((e: unknown) => {
        if (!cancelled)
          error =
            e instanceof ApiError && e.status === 404
              ? m.tasks_not_found()
              : errorText(e, m.tasks_task_load_failed());
      });
    return () => {
      cancelled = true;
    };
  });

  const canEdit = $derived(task !== undefined && task.access !== 'read');

  function currentInput(): TaskInput | undefined {
    if (form === undefined) return undefined;
    const estimate = estimateOf(estimateText);
    return {
      ...form,
      start_date: form.milestone || form.start_date === '' ? null : form.start_date,
      due_date: form.due_date === '' ? null : form.due_date,
      // 잘못된 입력은 저장 전에 막는다(submit). 여기서는 비교용으로만 쓴다.
      estimate_minutes: estimate === 'invalid' ? null : estimate,
      tags: tagsText
        .split(',')
        .map((t) => t.trim())
        .filter((t) => t !== ''),
    };
  }

  /** 저장: 서버 기준 내용과 날짜가 다르면 먼저 후속 업무 영향을 미리 보고, 옮길 것이 있으면 사용자에게 묻는다. */
  async function submit(base: TaskDetailView): Promise<void> {
    const input = currentInput();
    if (task === undefined || input === undefined || busy) return;
    // 잘못된 예상 시간을 빈 값으로 보내면 저장된 값이 지워진다. 입력을 그대로 두고 알린다.
    if (estimateOf(estimateText) === 'invalid') {
      error = m.tasks_estimate_invalid();
      return;
    }
    busy = true;
    error = undefined;
    saved = false;
    undatedNotice = '';
    try {
      // 목록의 선행 정보는 오래됐을 수 있으므로 날짜가 바뀌면 언제나 서버에 묻는다.
      let undated: UndatedTaskView[] = [];
      if (input.start_date !== base.start_date || input.due_date !== base.due_date) {
        const plan = await api.previewSchedule(task.id, input.start_date, input.due_date);
        if (plan.shifts.length > 0) {
          pending = {
            proposals: plan.shifts,
            undated: plan.undated,
            blocked: plan.blocked,
            input,
            baseVersion: base.version,
            retried: false,
          };
          return;
        }
        undated = plan.undated;
      }
      await save(input, base.version, [], undated);
    } catch (e: unknown) {
      error = errorText(e, m.tasks_save_failed());
    } finally {
      busy = false;
    }
  }

  /** 미리보기 패널의 선택. 승인은 패널을 띄울 때 붙잡아 둔 입력과 기준 버전 그대로 보낸다. */
  async function decide(withShifts: boolean): Promise<void> {
    const p = pending;
    if (p === undefined || busy) return;
    busy = true;
    error = undefined;
    try {
      await save(p.input, p.baseVersion, withShifts ? p.proposals : [], p.undated);
    } catch (e: unknown) {
      error = errorText(e, m.tasks_save_failed());
    } finally {
      busy = false;
    }
  }

  const toShift = (s: ShiftProposalView) => ({
    task_id: s.task_id,
    base_version: s.base_version,
    start_date: s.start_date,
    due_date: s.due_date,
  });

  async function save(
    input: TaskInput,
    baseVersion: number,
    shifts: ShiftProposalView[],
    undated: UndatedTaskView[] = [],
  ): Promise<void> {
    if (task === undefined) return;
    const result = await api.updateTask(task.id, baseVersion, input, shifts.map(toShift));
    if (result.kind === 'conflict') {
      pending = undefined;
      conflict = result.current;
      return;
    }
    if (result.kind === 'shift_conflict') {
      // 아무것도 바뀌지 않았다. 지금 상태로 다시 계산해 다시 묻는다(이제 옮길 것이 없으면 그대로 저장).
      const plan = await api.previewSchedule(task.id, input.start_date, input.due_date);
      if (plan.shifts.length === 0) {
        await save(input, baseVersion, [], plan.undated);
        return;
      }
      pending = {
        proposals: plan.shifts,
        undated: plan.undated,
        blocked: plan.blocked,
        input,
        baseVersion,
        retried: true,
      };
      return;
    }
    conflict = undefined;
    fill(result.task);
    undatedNotice = undatedText(undated);
    saved = true;
    onchanged();
    await focusSave();
  }

  // 패널이 사라질 때 대화상자의 초점 가두기가 첫 입력란으로 초점을 옮기므로, 그 뒤 프레임에서 요소로 돌린다.
  async function focusLater(id: string): Promise<void> {
    await tick();
    requestAnimationFrame(() => {
      document.getElementById(id)?.focus();
    });
  }

  async function focusSave(): Promise<void> {
    await focusLater('task-save');
  }

  async function reloadChecklist(next: Promise<unknown>): Promise<void> {
    await next;
    if (task === undefined) return;
    const fresh = await api.task(task.id);
    if (task.version === fresh.version) {
      task = fresh;
    } else {
      refresh(fresh);
    }
    onchanged();
  }

  async function saveRule(rule: RecurrenceRule | null): Promise<void> {
    if (task === undefined) return;
    const fresh = await api.setTaskRecurrence(
      task.id,
      rule,
      task.series?.timezone ?? localTimeZone(),
    );
    if (task.version === fresh.version) task = fresh;
    else refresh(fresh);
    onchanged();
  }

  /**
   * 선행 관계 지우기. 응답을 기다리는 동안은 다른 관계 저장을 막는다(늦게 온 지우기와 저장이 겹쳐 지운 관계를 다시
   * 만들지 않게). 고치던 관계를 지웠으면 편집 칸을 닫는다(그대로 두면 "관계 저장"이 지운 관계를 새로 만든다). 다른
   * 관계를 고치던 중이면 그 편집은 그대로 둔다. 실패하면 관계와 편집 내용을 그대로 두고 알린다.
   */
  async function removeLink(predecessor: string): Promise<void> {
    if (task === undefined || busy) return;
    busy = true;
    error = undefined;
    linkStatus = '';
    try {
      const fresh = await api.removePredecessor(task.id, predecessor);
      if (task.version === fresh.version) task = fresh;
      else refresh(fresh);
      if (editing === predecessor) resetLinkEditor();
      linkStatus = m.tasks_link_removed();
      onchanged();
      await focusLater('predecessors-heading');
    } catch (e: unknown) {
      error = errorText(e, m.tasks_predecessor_failed());
    } finally {
      busy = false;
    }
  }

  /** 지연일 글자: 0~상한의 정수만. */
  function lagOf(text: string): number | undefined {
    const t = text.trim();
    if (!/^\d+$/.test(t)) return undefined;
    const n = Number(t);
    return n <= MAX_LAG_DAYS ? n : undefined;
  }

  function startEdit(predecessor: string): void {
    const link = task?.predecessors.find((l) => l.task_id === predecessor);
    if (link === undefined) return;
    editing = predecessor;
    linkKind = link.kind;
    lagText = String(link.lag_days);
    linkBase = { kind: link.kind, lag_days: link.lag_days };
    linkStatus = '';
  }

  function resetLinkEditor(): void {
    editing = undefined;
    predecessorChoice = '';
    linkKind = 'FS';
    lagText = '0';
    linkBase = null;
  }

  /** 관계 저장: 먼저 서버에 영향을 묻고, 옮길 후속 업무가 있으면 사용자에게 보인다(D-31, D-91). */
  async function applyLink(predecessor: string): Promise<void> {
    if (task === undefined || busy || predecessor === '') return;
    // 고치던 관계가 그사이 없어졌으면(다시 읽은 판에 없음) 새로 만들지 않고 편집 칸을 닫는다.
    if (editing !== undefined && !task.predecessors.some((l) => l.task_id === editing)) {
      resetLinkEditor();
      return;
    }
    const lag = lagOf(lagText);
    if (lag === undefined) {
      error = m.tasks_link_lag_invalid();
      return;
    }
    busy = true;
    error = undefined;
    linkStatus = '';
    const kind = linkKind;
    const base = linkBase;
    try {
      const plan = await api.previewLink(task.id, predecessor, kind, lag);
      if (plan.shifts.length > 0) {
        linkPending = {
          predecessor,
          kind,
          lag,
          base,
          proposals: plan.shifts,
          undated: plan.undated,
          blocked: plan.blocked,
          retried: false,
        };
        return;
      }
      await saveLink(predecessor, kind, lag, base, [], plan.undated);
    } catch (e: unknown) {
      error = errorText(e, m.tasks_predecessor_failed());
    } finally {
      busy = false;
    }
  }

  async function saveLink(
    predecessor: string,
    kind: LinkKind,
    lag: number,
    base: LinkBase | null,
    shifts: ShiftProposalView[],
    undated: UndatedTaskView[],
  ): Promise<void> {
    if (task === undefined) return;
    const result = await api.setPredecessor(
      task.id,
      predecessor,
      kind,
      lag,
      base,
      shifts.map(toShift),
    );
    if (result.kind === 'link_conflict') {
      // 편집을 시작한 뒤 다른 곳에서 이 관계가 바뀌었다(아무것도 바뀌지 않음). 최신 관계를 보이고, 입력한 종류·지연일은
      // 그대로 두며, 다음 저장은 최신 관계를 기준으로 한다(다시 누르면 알고 덮는다).
      linkPending = undefined;
      const current = result.current;
      if (task.version === current.version) task = current;
      else refresh(current);
      const now = current.predecessors.find((l) => l.task_id === predecessor);
      if (now === undefined) {
        resetLinkEditor();
        error = m.tasks_link_gone();
      } else {
        editing = predecessor;
        linkBase = { kind: now.kind, lag_days: now.lag_days };
        error = m.tasks_link_changed({ now: linkText(now) });
      }
      onchanged();
      return;
    }
    if (result.kind === 'shift_conflict') {
      // 아무것도 바뀌지 않았다. 지금 상태로 다시 계산해 다시 묻는다(이제 옮길 것이 없으면 그대로 저장).
      const plan = await api.previewLink(task.id, predecessor, kind, lag);
      if (plan.shifts.length === 0) {
        await saveLink(predecessor, kind, lag, base, [], plan.undated);
        return;
      }
      linkPending = {
        predecessor,
        kind,
        lag,
        base,
        proposals: plan.shifts,
        undated: plan.undated,
        blocked: plan.blocked,
        retried: true,
      };
      return;
    }
    linkPending = undefined;
    resetLinkEditor();
    const fresh = result.task;
    if (task.version === fresh.version) task = fresh;
    else refresh(fresh);
    linkStatus = [m.tasks_link_saved(), undatedText(undated)].filter((s) => s !== '').join(' ');
    onchanged();
    // 패널과 편집 칸이 닫히므로 선행 업무 제목으로 돌린다.
    await focusLater('predecessors-heading');
  }

  async function decideLink(withShifts: boolean): Promise<void> {
    const p = linkPending;
    if (p === undefined || busy) return;
    busy = true;
    error = undefined;
    try {
      await saveLink(
        p.predecessor,
        p.kind,
        p.lag,
        p.base,
        withShifts ? p.proposals : [],
        p.undated,
      );
    } catch (e: unknown) {
      error = errorText(e, m.tasks_predecessor_failed());
    } finally {
      busy = false;
    }
  }

  async function changeParent(id: string, parentId: string | null): Promise<void> {
    error = undefined;
    parentStatus = '';
    try {
      const fresh = await api.setTaskParent(id, parentId);
      if (task?.version === fresh.version) task = fresh;
      else refresh(fresh);
      parentStatus = m.tasks_parent_changed();
      onchanged();
    } catch (e: unknown) {
      error = errorText(e, m.tasks_parent_failed());
    }
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
{#if task === undefined && error === undefined}
  <!-- 읽는 동안 빈 창으로 두지 않는다. -->
  <p class="py-6 text-center text-sm text-muted-foreground" role="status">{m.modules_loading()}</p>
{/if}
{#if task !== undefined && form !== undefined}
  {@const current = task}
  {@const f = form}
  <div class="flex flex-col gap-4">
    <div class="flex flex-wrap items-baseline gap-2">
      <span class="font-mono text-sm text-muted-foreground">{current.reference ?? ''}</span>
      {#if current.access === 'read'}<span class="text-sm text-muted-foreground"
          >{m.tasks_read_only()}</span
        >{/if}
      <div class="ml-auto flex flex-wrap items-center gap-1">
        <MarkButtons kind="task" id={current.id} />
        {#if canEdit}
          <DeleteButton
            what={m.tasks_noun_task()}
            remove={() => api.trashTask(current.id)}
            ondeleted={() => {
              onchanged();
              router.navigate(itemPath(current.workspace_id, current.item_id));
            }}
          />
        {/if}
      </div>
    </div>

    {#if conflict !== undefined}
      {@const latest = conflict}
      <div class="rounded-md border border-amber-500/50 p-3 text-sm" role="alert">
        <p>
          {m.tasks_conflict({ title: latest.title, status: STATUS_LABEL[latest.status] })}
        </p>
        <div class="mt-2 flex gap-2">
          <Button
            size="sm"
            variant="outline"
            onclick={() => {
              fill(latest);
              conflict = undefined;
            }}>{m.tasks_load_server()}</Button
          >
          <Button size="sm" disabled={busy} onclick={() => void submit(latest)}
            >{m.tasks_save_mine()}</Button
          >
        </div>
      </div>
    {/if}

    <!-- 제목 다음에 상태·우선순위·날짜를 두 칸으로 먼저 보이고(좁은 화면에서도 스크롤 없이), 설명은 그 아래에 둔다. -->
    <form
      class="grid grid-cols-2 gap-3"
      inert={pending !== undefined}
      onsubmit={(e) => {
        e.preventDefault();
        void submit(current);
      }}
    >
      <div class="col-span-2 flex flex-col gap-1">
        <Label for="task-title">{m.tasks_title()}</Label>
        <Input id="task-title" bind:value={f.title} disabled={!canEdit} maxlength={200} />
      </div>
      <div class="flex min-w-0 flex-col gap-1">
        <span class="text-sm font-medium" id="task-status-label">{m.tasks_status()}</span>
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
      <div class="flex min-w-0 flex-col gap-1">
        <span class="text-sm font-medium" id="task-priority-label">{m.tasks_priority()}</span>
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
      <div class="flex min-w-0 flex-col gap-1">
        <Label for="task-start">{m.tasks_start_date()}</Label>
        <Input
          id="task-start"
          type="date"
          bind:value={f.start_date}
          disabled={!canEdit || f.milestone}
        />
      </div>
      <div class="flex min-w-0 flex-col gap-1">
        <Label for="task-due">{m.tasks_due_date()}</Label>
        <Input id="task-due" type="date" bind:value={f.due_date} disabled={!canEdit} />
      </div>
      <div class="col-span-2 flex flex-col gap-1">
        <Label for="task-description">{m.tasks_description()}</Label>
        <textarea
          id="task-description"
          bind:value={f.description}
          disabled={!canEdit}
          rows={3}
          class="rounded-md border border-input bg-transparent px-3 py-2 text-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        ></textarea>
      </div>
      <div class="flex min-w-0 flex-col gap-1">
        <Label for="task-estimate">{m.tasks_estimate()}</Label>
        <Input
          id="task-estimate"
          inputmode="numeric"
          bind:value={estimateText}
          disabled={!canEdit}
        />
      </div>
      <div class="flex min-w-0 flex-col gap-1">
        <Label for="task-tags">{m.tasks_tags()}</Label>
        <Input id="task-tags" bind:value={tagsText} disabled={!canEdit} />
      </div>
      <div class="col-span-2 flex items-center gap-2">
        <Checkbox
          id="task-milestone"
          checked={f.milestone}
          disabled={!canEdit}
          onCheckedChange={(v) => {
            f.milestone = v === true;
            if (f.milestone) f.start_date = null;
          }}
        />
        <Label for="task-milestone">{m.tasks_milestone()}</Label>
      </div>
      <fieldset class="col-span-2 flex flex-col gap-2">
        <legend class="text-sm font-medium">{m.tasks_assignees()}</legend>
        <div class="flex flex-wrap gap-3">
          {#if peopleError !== undefined}
            <span class="text-sm text-destructive" role="alert">{peopleError}</span>
          {:else if people === undefined}
            <span class="text-sm text-muted-foreground" role="status">{m.modules_loading()}</span>
          {:else}
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
              <span class="text-sm text-muted-foreground">{m.tasks_no_assignable()}</span>
            {/each}
          {/if}
        </div>
      </fieldset>
      {#if canEdit}
        <div class="col-span-2 flex items-center gap-2">
          <Button id="task-save" type="submit" disabled={busy}>{m.common_save()}</Button>
          {#if saved}<span class="text-sm" role="status">{m.common_saved()}</span>{/if}
        </div>
        {#if undatedNotice !== ''}
          <p class="col-span-2 text-sm text-muted-foreground" data-testid="task-undated-notice">
            {undatedNotice}
          </p>
        {/if}
      {/if}
    </form>

    {#if pending !== undefined}
      <ShiftProposals
        proposals={pending.proposals}
        undated={pending.undated}
        blocked={pending.blocked}
        retried={pending.retried}
        {busy}
        ondecide={(withShifts: boolean) => void decide(withShifts)}
        oncancel={() => {
          pending = undefined;
          void focusSave();
        }}
      />
    {/if}

    <section class="flex flex-col gap-2" aria-labelledby="parent-heading">
      <h3 id="parent-heading" class="text-sm font-semibold">{m.tasks_parent()}</h3>
      {#if current.parent_id !== null}
        {@const parentId = current.parent_id}
        <button
          type="button"
          class="self-start rounded px-2 py-1 text-left text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          onclick={() => {
            onopen(parentId);
          }}>{label(byId.get(parentId)) || m.tasks_hidden_task()}</button
        >
      {:else}
        <p class="text-sm text-muted-foreground">{m.tasks_parent_top()}</p>
      {/if}
      {#if canEdit && subtasks.length > 0}
        <p class="text-sm text-muted-foreground">{m.tasks_parent_has_subtasks()}</p>
      {:else if canEdit && (parentCandidates.length > 0 || current.parent_id !== null)}
        <div class="flex flex-wrap items-center gap-2">
          <span id="parent-choose-label" class="sr-only">{m.tasks_parent_choose()}</span>
          <Select.Root
            type="single"
            value={parentChoice}
            onValueChange={(v: string) => (parentChoice = v)}
          >
            <Select.Trigger aria-labelledby="parent-choose-label"
              >{parentChoice === ''
                ? m.tasks_parent_choose()
                : parentChoice === TOP
                  ? m.tasks_parent_top()
                  : label(byId.get(parentChoice))}</Select.Trigger
            >
            <Select.Content>
              {#if current.parent_id !== null}
                <Select.Item value={TOP} label={m.tasks_parent_top()}
                  >{m.tasks_parent_top()}</Select.Item
                >
              {/if}
              {#each parentCandidates as c (c.id)}
                <Select.Item value={c.id} label={label(c)}>{label(c)}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
          <Button
            size="sm"
            disabled={parentChoice === ''}
            onclick={() => {
              const chosen = parentChoice;
              parentChoice = '';
              void changeParent(current.id, chosen === TOP ? null : chosen);
            }}>{m.tasks_parent_change()}</Button
          >
        </div>
      {/if}
      <p aria-live="polite" class="text-sm">{parentStatus}</p>
    </section>

    {#if current.parent_id === null}
      <section class="flex flex-col gap-2" aria-labelledby="subtasks-heading">
        <h3 id="subtasks-heading" class="text-sm font-semibold">{m.tasks_subtasks()}</h3>
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
          <p class="text-sm text-muted-foreground">{m.tasks_none()}</p>
        {/each}
        {#if canEdit}
          <CreateForm
            id="new-subtask"
            label={m.tasks_subtask_title()}
            placeholder={m.tasks_subtask_placeholder()}
            submitLabel={m.tasks_subtask_add()}
            create={async (requestId: string, title: string) => {
              await api.createTask(current.item_id, requestId, title, current.id);
              onchanged();
            }}
          />
        {/if}
      </section>
    {/if}

    <section class="flex flex-col gap-2" aria-labelledby="predecessors-heading">
      <h3 id="predecessors-heading" class="text-sm font-semibold" tabindex="-1">
        {m.tasks_predecessors()}
      </h3>
      {#each current.predecessors as link (link.task_id)}
        {@const pid = link.task_id}
        <div class="flex flex-wrap items-center gap-x-2 text-sm" data-predecessor={pid}>
          <button
            type="button"
            class="rounded px-2 py-1 text-left hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            onclick={() => {
              onopen(pid);
            }}>{label(byId.get(pid)) || m.tasks_hidden_task()}</button
          >
          <span class="text-muted-foreground" data-link-text>{linkText(link)}</span>
          {#if canEdit}
            <span class="ml-auto flex gap-1">
              <Button
                size="sm"
                variant="ghost"
                disabled={busy || linkPending !== undefined}
                aria-label={m.tasks_link_edit_aria({ task: label(byId.get(pid)) })}
                onclick={() => {
                  startEdit(pid);
                }}>{m.tasks_link_edit()}</Button
              >
              <Button
                size="sm"
                variant="ghost"
                disabled={busy || linkPending !== undefined}
                aria-label={m.tasks_predecessor_remove_aria({ task: label(byId.get(pid)) })}
                onclick={() => void removeLink(pid)}>{m.tasks_remove()}</Button
              >
            </span>
          {/if}
        </div>
      {:else}
        <p class="text-sm text-muted-foreground">{m.tasks_none()}</p>
      {/each}
      {#if canEdit && (editing !== undefined || candidates.length > 0)}
        <fieldset
          class="flex flex-col gap-2 rounded-md border p-2"
          inert={linkPending !== undefined}
          aria-describedby="link-kind-meaning"
        >
          <legend class="px-1 text-sm font-medium" id="predecessor-label">
            {editing === undefined ? m.tasks_predecessor_add() : m.tasks_link_edit()}
          </legend>
          {#if editing !== undefined}
            <p class="text-sm">{m.tasks_link_editing({ task: label(byId.get(editing)) })}</p>
          {:else}
            <Select.Root
              type="single"
              value={predecessorChoice}
              onValueChange={(v: string) => (predecessorChoice = v)}
            >
              <Select.Trigger aria-labelledby="predecessor-label"
                >{predecessorChoice === ''
                  ? m.tasks_predecessor_choose()
                  : label(byId.get(predecessorChoice))}</Select.Trigger
              >
              <Select.Content>
                {#each candidates as c (c.id)}
                  <Select.Item value={c.id} label={label(c)}>{label(c)}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
          {/if}
          <div class="flex flex-wrap items-end gap-2">
            <div class="flex min-w-0 flex-col gap-1">
              <span id="link-kind-label" class="text-sm">{m.tasks_link_kind_label()}</span>
              <Select.Root
                type="single"
                value={linkKind}
                onValueChange={(v: string) => {
                  const found = LINK_KINDS.find((k) => k === v);
                  if (found !== undefined) linkKind = found;
                }}
              >
                <Select.Trigger aria-labelledby="link-kind-label" data-testid="link-kind"
                  >{LINK_KIND_LABEL[linkKind]}</Select.Trigger
                >
                <Select.Content>
                  {#each LINK_KINDS as k (k)}
                    <Select.Item value={k} label={LINK_KIND_LABEL[k]}
                      >{LINK_KIND_LABEL[k]}</Select.Item
                    >
                  {/each}
                </Select.Content>
              </Select.Root>
            </div>
            <div class="flex w-28 flex-col gap-1">
              <Label for="link-lag">{m.tasks_link_lag_label()}</Label>
              <Input
                id="link-lag"
                inputmode="numeric"
                aria-describedby="link-lag-hint"
                bind:value={lagText}
              />
            </div>
          </div>
          <p id="link-kind-meaning" class="text-sm text-muted-foreground">
            {LINK_KIND_MEANING[linkKind]}
          </p>
          <p id="link-lag-hint" class="text-xs text-muted-foreground">{m.tasks_link_lag_hint()}</p>
          <div class="flex flex-wrap gap-2">
            <Button
              size="sm"
              disabled={busy || (editing === undefined && predecessorChoice === '')}
              id="link-save"
              onclick={() => void applyLink(editing ?? predecessorChoice)}
              >{m.tasks_link_save()}</Button
            >
            {#if editing !== undefined}
              <Button size="sm" variant="ghost" disabled={busy} onclick={resetLinkEditor}
                >{m.common_cancel()}</Button
              >
            {/if}
          </div>
        </fieldset>
      {/if}
      {#if linkPending !== undefined}
        <ShiftProposals
          proposals={linkPending.proposals}
          undated={linkPending.undated}
          blocked={linkPending.blocked}
          retried={linkPending.retried}
          {busy}
          forLink
          headingId="link-shift-heading"
          ondecide={(withShifts: boolean) => void decideLink(withShifts)}
          oncancel={() => {
            // 편집 칸은 그대로 두고 관계 저장 단추로 돌린다(고친 값으로 다시 시도할 수 있게).
            linkPending = undefined;
            void focusLater('link-save');
          }}
        />
      {/if}
      <p aria-live="polite" class="text-sm" data-testid="link-status">{linkStatus}</p>
    </section>

    <section class="flex flex-col gap-2" aria-labelledby="successors-heading">
      <h3 id="successors-heading" class="text-sm font-semibold">{m.tasks_successors_heading()}</h3>
      {#each successors as s (s.task.id)}
        <div class="flex flex-wrap items-center gap-x-2 text-sm" data-successor={s.task.id}>
          <button
            type="button"
            class="rounded px-2 py-1 text-left hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            onclick={() => {
              onopen(s.task.id);
            }}>{label(s.task)}</button
          >
          <span class="text-muted-foreground" data-link-text>{linkText(s.link)}</span>
        </div>
      {:else}
        <p class="text-sm text-muted-foreground">{m.tasks_none()}</p>
      {/each}
    </section>

    {#if current.parent_id === null}
      {#if current.series !== null && current.series.root_task_id !== current.id}
        {@const rootId = current.series.root_task_id}
        <section class="flex flex-col gap-2" aria-labelledby="task-recurrence-heading">
          <h3 id="task-recurrence-heading" class="text-sm font-semibold">
            {m.tasks_recurrence()}
          </h3>
          <p class="text-sm text-muted-foreground">
            {m.tasks_occurrence_note({ date: current.occurrence_date ?? '' })}
          </p>
          <div>
            <Button
              size="sm"
              variant="outline"
              onclick={() => {
                onopen(rootId);
              }}>{m.tasks_open_first()}</Button
            >
          </div>
        </section>
      {:else}
        <RecurrenceEditor
          idPrefix="task-recurrence"
          rule={current.series?.rule ?? null}
          canEdit={canEdit && (current.due_date !== null || current.start_date !== null)}
          zoneNote={m.tasks_recurrence_zone({
            zone: current.series?.timezone ?? localTimeZone(),
          })}
          onsave={saveRule}
        />
        {#if canEdit && current.due_date === null && current.start_date === null}
          <p class="text-sm text-muted-foreground">{m.tasks_recurrence_needs_date()}</p>
        {/if}
      {/if}
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
      ontoggle={(id: string, done: boolean) =>
        reloadChecklist(api.updateChecklistItem(id, { checked: done }))}
      onrename={(id: string, text: string) =>
        reloadChecklist(api.updateChecklistItem(id, { text }))}
      onremove={(id: string) => reloadChecklist(api.deleteChecklistItem(id))}
    />

    <TaskTimePanel taskId={current.id} />
    <CommentsPanel target={{ kind: 'task', id: current.id }} />

    <Backlinks kind="task" id={current.id} {workspaces} />
  </div>
{/if}
