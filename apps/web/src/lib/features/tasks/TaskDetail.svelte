<script lang="ts">
  import TaskHierarchy from './TaskHierarchy.svelte';
  import TaskFields from './TaskFields.svelte';
  import TaskDependencies from './TaskDependencies.svelte';
  import TaskRecurrence from './TaskRecurrence.svelte';
  import TaskSidePanels from './TaskSidePanels.svelte';
  import type { LinkPending } from './detail-state';
  import { parseTaskDraft, type TaskDraft } from './detail-draft';
  import { taskData as work } from '$lib/features/tasks/detail-data';
  import {
    taskScreenApi,
    composeScreenDetail,
    type DetailRead,
    type DetailOptionsRead,
  } from './screen-api';
  import {
    DiscardedTaskRead,
    readTaskSessionGeneration,
    sameContext,
    type CapturedRead,
    type ContextCapture,
  } from './context-read';
  import {
    TASK_SESSION_RECOVERY,
    TaskSessionUnsettled,
    readTaskWithRecovery,
    type TaskSessionRecovery,
  } from './session-recovery';
  // 업무 상세 (D-31): 필드·담당자·하위 업무·체크리스트·역방향 링크.
  // 필드 저장은 기준 버전으로 충돌을 막는다. 충돌하면 입력을 지우지 않고 서버 내용과 함께 보여 준다(D-53과 같은 원칙).
  import { getContext, onDestroy, tick, untrack } from 'svelte';
  import { me } from '$lib/me.svelte';

  import type { TaskStatusDefinition, Workspace } from '@fvoci/api-client';

  import type {
    AssigneeView,
    LinkBase,
    LinkKind,
    RecurrenceRule,
    ShiftProposalView,
    TaskDetailView,
    TaskInput,
    TaskView,
    PlanBlockedView,
    UndatedTaskView,
  } from '$lib/api/client';
  import { api, ApiError, TaskRuleConflict, MAX_LAG_DAYS } from '$lib/api/client';
  import {
    accountSafety,
    accountSwitching,
    notifyDone,
  } from '$lib/features/feedback/feedback.svelte';
  import { errorText } from '$lib/api/errors';
  import type { NewChecklistEntry } from '$lib/features/checklists/checklist-item';
  import type { StoredFormDraft } from '$lib/features/tasks/drafts';
  import {
    clearFormDraft,
    formDraftTabKey,
    holdFormDraft,
    listFormDrafts,
    removeFormDraftAt,
    storeFormDraft,
  } from '$lib/features/tasks/drafts';
  import { formatInstant } from '$lib/display';
  import ChecklistForm from '$lib/features/checklists/ChecklistForm.svelte';

  import MarkButtons from '$lib/features/home/MarkButtons.svelte';
  import DeleteButton from '$lib/features/trash/DeleteButton.svelte';
  import { itemPath, router } from '$lib/router.svelte';
  import { localTimeZone } from '$lib/features/events/time';

  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  import FieldConflict from '$lib/features/common/FieldConflict.svelte';
  import {
    fieldDifferences,
    initialFieldChoices,
    selectedFields,
  } from '$lib/features/common/field-conflict';
  import { taskConflictFields, type TaskComparison } from './field-conflict';
  import type { TaskConflictCurrent } from './model';
  import { createTaskForm, type Merge } from './task-form.svelte';
  import { changedTaskFields, edited, estimateOf, formOf } from './form';
  import { assigneeLabels, linkText, undatedText } from './labels';
  import ShiftProposals from './ShiftProposals.svelte';

  interface Props {
    aggregate?: boolean;
    initialRead?: ((taskId: string) => CapturedRead<DetailRead> | undefined) | undefined;
    readOnly?: boolean;
    taskId: string;
    /** 부모 항목의 읽기 힌트. 실제 업무의 item_id가 다르면 조합층이 다시 읽는다. */
    itemId?: string | undefined;
    /** 같은 항목의 업무 전체(하위 업무·선행 후보·후속 확인용). */
    tasks: TaskView[];
    workspaces: Workspace[];
    onchanged: () => void;
    onopen: (taskId: string) => void;
    /** 기준과 다르게 고친 칸이 있는지 바뀔 때마다 알린다(대화상자가 닫기 전에 묻는다). */
    onunsaved?: ((unsaved: boolean) => void) | undefined;
    /** 폼을 저장하는 중인지 바뀔 때마다 알린다(대화상자는 그동안 닫지 않는다). */
    onsaving?: ((saving: boolean) => void) | undefined;
    /**
     * 고친 입력을 이 탭의 브라우저 초안으로 남겼는지 바뀔 때마다 알린다(남기지 못했으면 — 저장소 사용 불가·용량 초과 — 계정 전환
     * 중에도 떠나기 경고를 유지해 입력을 잃지 않게 한다, D-96). 고친 칸이 없으면 true.
     */
    ondraftkept?: ((kept: boolean) => void) | undefined;
  }

  const {
    aggregate = false,
    initialRead,
    taskId,
    itemId,
    tasks,
    workspaces,
    onchanged,
    onopen,
    onunsaved,
    onsaving,
    ondraftkept,
    readOnly = false,
  }: Props = $props();

  const editingUser = untrack(() => me.userId);
  const sessionRecovery = getContext<TaskSessionRecovery | undefined>(TASK_SESSION_RECOVERY);
  interface ChildEditor {
    discardInput(): boolean;
    isSaving(): boolean;
  }
  let commentsEditor = $state<ChildEditor>();
  let checklistEditor = $state<ChildEditor>();
  let recurrenceEditor = $state<{ discardDraft: () => void; isSaving: () => boolean }>();
  let recurrenceSaving = $state(false);
  const childSaving = $derived(
    commentsEditor?.isSaving() === true ||
      checklistEditor?.isSaving() === true ||
      recurrenceSaving ||
      recurrenceEditor?.isSaving() === true,
  );

  export function discardChildInput(): boolean {
    if (childSaving || saving) return false;
    commentsEditor?.discardInput();
    checklistEditor?.discardInput();
    recurrenceEditor?.discardDraft();
    return true;
  }
  let accountBlocked = $state(false);
  const hiddenForAccount = $derived(accountBlocked || me.userId !== editingUser);
  async function verifyAccount(): Promise<void> {
    const current = await api.me().catch(() => undefined);
    if (current?.id === editingUser) accountBlocked = false;
  }
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

  let definitions = $state<TaskStatusDefinition[]>([]);
  let screenArchived = $state<boolean | undefined>();
  $effect(() => {
    if (aggregate) return;
    const itemId = detailForm.task?.item_id;
    if (itemId === undefined) return;
    let cancelled = false;
    void work
      .taskStatuses(itemId)
      .then((list) => {
        if (!cancelled) definitions = [...list].sort((a, b) => a.position - b.position);
      })
      .catch((e: unknown) => {
        if (!cancelled) error = errorText(e, m.tasks_load_failed());
      });
    return () => {
      cancelled = true;
    };
  });
  const statusNames = $derived(definitions.map((s) => s.name));
  const detailForm = createTaskForm();
  const { mergeOnto, inputOf, currentInput, toggleAssignee } = detailForm;
  // 지정할 수 있는 사람. 받기 전(undefined)에는 "없습니다"를 보이지 않는다.
  let people = $state<AssigneeView[] | undefined>(undefined);
  // 지정할 수 있는 사람을 받지 못했을 때의 안내(업무는 받았으므로 업무 불러오기 실패와 따로 알린다).
  let peopleError = $state<string | undefined>(undefined);
  let comparison = $state<TaskComparison>();
  let preservedComparison = $state<TaskComparison>();
  let permissionBlocked = $state(false);
  let hiddenForAccess = $state(false);
  const conflictFields = $derived(
    taskConflictFields(assigneeLabels([...(detailForm.task?.assignees ?? []), ...(people ?? [])])),
  );
  let error = $state<string | undefined>(undefined);
  let retryRead = $state<(() => void) | undefined>();
  let readAttempt = $state(0);
  let consumedAttempt = 0;
  // 날짜를 바꿀 때 함께 옮길 후속 업무(미리보기). 패널이 떠 있는 동안 폼은 잠그고, 승인하면 본 입력과 기준 버전
  // 그대로 보낸다(D-31: 보여 준 영향을 사용자가 승인).
  interface Pending {
    proposals: ShiftProposalView[];
    undated: UndatedTaskView[];
    blocked: PlanBlockedView | null;
    input: TaskInput;
    patch: Partial<TaskInput>;
    baseVersion: number;
    retried: boolean;
  }
  let pending = $state<Pending | undefined>(undefined);
  let busy = $state(false);
  let predecessorChoice = $state('');
  // 관계 고치기(D-91): 고르는 선행 업무(새 관계) 또는 고치는 관계, 종류, 지연일(글자 그대로 두고 저장할 때 확인).
  let editing = $state<string | undefined>(undefined);
  let linkKind = $state<LinkKind>('FS');
  let lagText = $state('0');
  // 편집을 시작한 관계의 종류·지연일(새 관계면 null). 저장할 때 함께 보내 그사이 다른 곳에서 바뀌었으면 서버가 409로
  // 거절한다(말없이 덮지 않음).
  let linkBase = $state<LinkBase | null>(null);
  const linkChanged = $derived(
    editing === undefined
      ? predecessorChoice !== '' || linkKind !== 'FS' || lagText !== '0'
      : linkKind !== linkBase?.kind || lagText !== String(linkBase.lag_days),
  );
  // 관계를 바꾸면 옮겨야 하는 후속 업무(미리보기). 승인하면 본 관계·목록 그대로 보낸다.

  let linkPending = $state<LinkPending | undefined>(undefined);
  // 상위 업무 고르기. 'top'은 최상위로 빼기. 서버 규칙(한 단계, 하위 업무가 있으면 옮길 수 없음)에 맞는 후보만 보인다.
  let parentChoice = $state('');
  let childDraft = $state<{ name: string; requestId: string | null } | undefined>();
  let checklistUnsaved = $state(false);
  let commentsUnsaved = $state(false);
  let recurrenceUnsaved = $state(false);
  let checklistDraftKept = $state(true);
  let commentsDraftKept = $state(true);
  let recurrenceDraftKept = $state(true);
  let taskDraftKept = $state(true);
  const parentCandidates = $derived(
    tasks.filter(
      (t) => t.id !== taskId && t.parent_id === null && t.id !== detailForm.task?.parent_id,
    ),
  );
  const candidates = $derived(
    detailForm.task === undefined
      ? []
      : tasks.filter(
          (t) =>
            t.id !== taskId &&
            !(detailForm.task?.predecessors.some((p) => p.task_id === t.id) ?? false),
        ),
  );

  // 다시 채울 때 비교할 기준(마지막으로 다 불러온 판의 값, 사용자가 바꾸지 않은 칸만 새 값으로 옮긴다)과, 같은 칸을
  // 양쪽에서 바꿔 충돌이 있었는지(다음 저장·불러오기까지 지킨다), 그때 남겨 둘 옛 판 번호. 기준은 저장하지 않은 입력이
  // 있는지(`unsaved`)도 정하므로 상태로 둔다.

  function fill(next: TaskDetailView): void {
    detailForm.fill(next);
    pending = undefined;
    comparison = undefined;
    preservedComparison = undefined;
    permissionBlocked = next.access === 'read';
    hiddenForAccess = false;
  }
  function applyMerge(merge: Merge): void {
    detailForm.applyMerge(merge);
    pending = undefined;
    comparison = undefined;
  }

  /**
   * 체크리스트·반복·선행 업무를 바꾼 뒤 서버의 새 판으로 다시 채운다. 고쳤지만 저장하지 않은 칸(태그·예상 시간은
   * 저장할 때 보내는 값으로 비교)은 지키고, 그 칸을 서버에서도 누가 바꿨으면 옛 판 번호를 남겨 저장할 때 충돌
   * 화면이 고르게 한다.
   */
  function refresh(fresh: TaskDetailView): void {
    if (comparison !== undefined) {
      // A composed detail may mix independently read relations. Only a 409 can replace the reviewed snapshot.
      detailForm.task = { ...fresh, version: detailForm.baseVersion };
      return;
    }
    const mine = currentInput();
    if (detailForm.task === undefined || mine === undefined || detailForm.baseline === undefined) {
      fill(fresh);
      return;
    }
    const merge = mergeOnto(fresh, mine, detailForm.baseline);
    detailForm.baseline = merge.baseline;
    // 충돌은 지금의 병합으로 다시 본다. 기준은 사용자가 바꾼 칸을 옛 값으로 지키므로, 서버가 그 칸을 다르게 두는 한 계속
    // 충돌이고, 사용자가 서버 값으로 맞추면 풀려 최신 판으로 저장한다.
    detailForm.clashed = merge.clashes.length > 0;
    if (!detailForm.clashed) detailForm.baseVersion = fresh.version;
    detailForm.task = detailForm.clashed ? { ...fresh, version: detailForm.baseVersion } : fresh;
    applyMerge(merge);
  }

  // 폼의 담당자 이름표. 충돌 확인과 같은 글이라 확인에 보인 사람을 폼에서 찾을 수 있다.
  const peopleLabels = $derived(
    assigneeLabels([...(detailForm.task?.assignees ?? []), ...(people ?? [])]),
  );

  /** Compare against the local edit baseline; a 409's changed list is not a three-way diff. */
  function openComparison(
    latest: Pick<TaskConflictCurrent, keyof TaskInput | 'version'>,
    keptChoices?: TaskComparison['choices'],
  ): boolean {
    const mine = currentInput();
    const base = detailForm.baseline;
    if (mine === undefined || base === undefined) return false;
    const server: TaskInput = {
      title: latest.title,
      description: latest.description,
      status: latest.status,
      priority: latest.priority,
      start_date: latest.start_date ?? null,
      due_date: latest.due_date ?? null,
      milestone: latest.milestone,
      estimate_minutes: latest.estimate_minutes ?? null,
      tags: latest.tags,
      assignee_ids: latest.assignee_ids,
    };
    const old = preservedComparison;
    const next: TaskComparison = {
      base: $state.snapshot(base),
      mine: $state.snapshot(mine),
      server,
      version: latest.version,
      baseVersion: detailForm.baseVersion,
      tagsText: detailForm.tagsText,
      estimateText: detailForm.estimateText,
      original: old?.original ?? $state.snapshot(mine),
      originalBase: old?.originalBase ?? $state.snapshot(base),
      originalVersion: old?.originalVersion ?? detailForm.baseVersion,
      originalTagsText: old?.originalTagsText ?? detailForm.tagsText,
      originalEstimateText: old?.originalEstimateText ?? detailForm.estimateText,
      choices:
        keptChoices === undefined
          ? initialFieldChoices(fieldDifferences(conflictFields, base, mine, server))
          : $state.snapshot(keptChoices),
      repeated: old !== undefined,
    };
    comparison = next;
    preservedComparison = next;
    return true;
  }

  /** Keep the original input while saving the chosen values against the reviewed version. */
  function leaveComparison(original: boolean): void {
    const kept = comparison;
    if (kept === undefined || busy) return;
    detailForm.form = $state.snapshot(original ? kept.original : kept.mine);
    detailForm.tagsText = original ? kept.originalTagsText : kept.tagsText;
    detailForm.estimateText = original ? kept.originalEstimateText : kept.estimateText;
    detailForm.baseline = original ? kept.originalBase : kept.base;
    detailForm.baseVersion = original ? kept.originalVersion : kept.baseVersion;
    comparison = undefined;
    void focusLater('task-save');
  }
  async function applyChoices(saveNow: boolean): Promise<void> {
    const ask = comparison;
    const task = detailForm.task;
    if (ask === undefined || task === undefined || busy || !canEdit) return;
    const result = selectedFields(
      conflictFields,
      $state.snapshot(ask.mine),
      $state.snapshot(ask.server),
      ask.choices,
    );
    preservedComparison = $state.snapshot(ask);
    detailForm.form = result;
    detailForm.tagsText = result.tags.join(', ');
    detailForm.estimateText =
      result.estimate_minutes === null ? '' : String(result.estimate_minutes);
    detailForm.baseline = ask.server;
    detailForm.baseVersion = ask.version;
    detailForm.clashed = false;
    if (saveNow) {
      if (!edited(ask.server, result)) {
        comparison = undefined;
        preservedComparison = undefined;
        notifyDone(m.common_conflict_equal(), 'task-save');
        await focusLater('task-save');
      } else {
        const captured = writeContext();
        await submit();
        if (!acceptsWrite(captured)) return;
        if (pending !== undefined) {
          comparison = undefined;
          await focusLater('shift-heading');
        }
      }
    } else {
      comparison = undefined;
      await focusLater('task-title');
    }
  }

  let detailReadSequence = 0;
  $effect(() => {
    const id = taskId;
    const retrying = readAttempt !== consumedAttempt;
    consumedAttempt = readAttempt;
    const navigation = untrack(() => aggregate) ? router.navigationGeneration : undefined;
    const parentItemId = untrack(() => itemId);
    const sequence = ++detailReadSequence;
    busy = false;
    saving = false;
    const capture = (): ContextCapture => ({
      userId: me.userId,
      sequence: detailReadSequence,
      navigationGeneration: router.navigationGeneration,
      sessionGeneration: readTaskSessionGeneration(),
      safetyGeneration: accountSafety.generation,
      path: router.path,
    });
    let snapshot = { ...untrack(capture), sequence };
    let cancelled = false;
    const isCurrent = (): boolean =>
      !cancelled &&
      (navigation === undefined ||
        (navigation === router.navigationGeneration && sameContext(snapshot, capture())));
    const active = (): boolean =>
      !cancelled &&
      navigation === router.navigationGeneration &&
      !accountSafety.switching &&
      !accountSafety.restoring &&
      !hiddenForAccount;
    detailForm.task = undefined;
    latestRule = undefined;
    comparison = undefined;
    error = undefined;
    retryRead = undefined;
    screenArchived = undefined;
    const read = untrack(() =>
      aggregate
        ? retrying
          ? Promise.reject<DetailRead>(new DiscardedTaskRead())
          : initialRead?.(id)?.take()
        : undefined,
    );
    const failed = (e: unknown): void => {
      if (isCurrent() && !(e instanceof DiscardedTaskRead))
        error =
          e instanceof ApiError && e.status === 404
            ? m.tasks_not_found()
            : errorText(e, m.tasks_task_load_failed());
    };
    const apply = (detail: TaskDetailView, aggregateRead?: DetailRead): void => {
      if (!isCurrent()) return;
      // 되살릴지 물을 입력은 채우기 전에 고른다(이 탭의 키에 남은 것을 입력 보존 효과가 지우기 전에 옮긴다).
      offerDraft(detail);
      fill(detail);
      peopleError = undefined;
      if (aggregateRead !== undefined) {
        screenArchived = aggregateRead.archived;
        definitions = [];
        people = undefined;
        const loadOptions = (initial?: Promise<DetailOptionsRead>): void => {
          retryRead = undefined;
          void readTaskWithRecovery(
            snapshot,
            capture,
            active,
            sessionRecovery,
            () => taskScreenApi.options(detail.item_id),
            initial,
            {
              loaded: (options, captured) => {
                snapshot = captured;
                if (!isCurrent()) return;
                if (options.statuses.state === 'ready') definitions = options.statuses.value;
                else error = m.tasks_load_failed();
                if (options.assignable.state === 'ready') people = options.assignable.value;
                else peopleError = m.tasks_assignable_load_failed();
              },
              failed: (cause, captured) => {
                snapshot = captured;
                if (!isCurrent() || cause instanceof DiscardedTaskRead) return;
                if (cause instanceof ApiError && [401, 403, 404].includes(cause.status)) {
                  detailForm.task = undefined;
                  error = errorText(cause, m.tasks_task_load_failed());
                } else {
                  error = m.tasks_load_failed();
                  peopleError = m.tasks_assignable_load_failed();
                }
                if (cause instanceof TaskSessionUnsettled)
                  retryRead = () => {
                    error = undefined;
                    peopleError = undefined;
                    loadOptions(Promise.reject<DetailOptionsRead>(new DiscardedTaskRead()));
                  };
              },
            },
          ).catch(() => undefined);
        };
        loadOptions();
        return;
      }
      void work.assignable(detail.item_id).then(
        (assignable) => {
          if (!cancelled) people = assignable;
        },
        (e: unknown) => {
          if (cancelled) return;
          people = undefined;
          peopleError = errorText(e, m.tasks_assignable_load_failed());
        },
      );
    };
    if (aggregate) {
      void readTaskWithRecovery(
        snapshot,
        capture,
        active,
        sessionRecovery,
        () => taskScreenApi.detail(id),
        read,
        {
          loaded: (value, captured) => {
            snapshot = captured;
            apply(composeScreenDetail(value), value);
          },
          failed: (cause, captured) => {
            snapshot = captured;
            failed(cause);
            if (cause instanceof TaskSessionUnsettled && isCurrent())
              retryRead = () => {
                readAttempt += 1;
              };
          },
        },
      ).catch(() => undefined);
    } else
      void work
        .task(id, parentItemId)
        .then((detail) => {
          apply(detail);
        })
        .catch(failed);
    return () => {
      cancelled = true;
    };
  });

  const canEdit = $derived(
    !hiddenForAccount &&
      !hiddenForAccess &&
      !accountSafety.switching &&
      !accountSafety.restoring &&
      !readOnly &&
      !permissionBlocked &&
      detailForm.task !== undefined &&
      detailForm.task.access !== 'read',
  );
  // 폼을 저장하는 동안은 폼 칸을 잠근다: 저장되면 폼을 저장된 판으로 다시 채우므로(`fill`) 그사이 친 글이 말없이
  // 사라졌다. 관계·상위 업무 요청은 돌아오면 고친 칸을 지켜 다시 채우므로(`refresh`) 잠그지 않는다.
  let saving = $state(false);
  const locked = $derived(!canEdit || saving);

  // 업무 대화상자가 닫기 전에 버릴지 물을 입력이 있는지(편집할 수 있을 때만).
  const unsaved = $derived(
    !hiddenForAccount &&
      (detailForm.changed ||
        preservedComparison !== undefined ||
        linkChanged ||
        parentChoice !== '' ||
        childDraft !== undefined ||
        checklistUnsaved ||
        commentsUnsaved ||
        recurrenceUnsaved),
  );
  $effect(() => {
    onunsaved?.(unsaved);
    return () => {
      onunsaved?.(false);
    };
  });
  $effect(() => {
    onsaving?.(saving || childSaving);
    return () => {
      onsaving?.(false);
    };
  });

  // 저장하지 않은 입력의 브라우저 보존(D-96·D-128; 문서 초안 D-27과 같은 규칙, common/form-draft.ts). 입력이 기준과 다르면
  // 이 사용자·이 탭의 키에 남기고 같아지면 지운다. 다시 읽은 뒤(새로고침, 다른 탭에서 계정이 바뀌어 다시 읽음) 같은 계정이
  // 이 업무를 열면 되살릴지 묻는다. 다른 계정은 키가 달라 보지 못하고, 되살린 입력은 사용자가 "저장"을 눌러야 간다(자동
  // 재전송 없음). 하위 업무 제목은 초안에, 관계·상위 선택은 화면의 이탈 확인에만 포함한다(D-27·D-97).
  const draftName = $derived(`task:${taskId}`);
  let restorable = $state<StoredFormDraft<TaskDraft> | undefined>(undefined);
  let childInitial = $state<{ name: string; requestId: string | null } | undefined>();
  let childSeed = $state(0);
  // 묻는 입력이 보류 키로 옮기지 못해 이 탭의 키에 그대로 있으면(저장소 가득 참) 고를 때까지 그 키에 새 입력을 쓰지 않는다.
  let reserved = false;
  /** 서버 내용과 같은(이미 반영된) 입력인지. 관계 편집 칸에 입력이 남아 있으면 같지 않다. */
  const isServer = (d: TaskDraft, server: TaskDetailView): boolean =>
    d.child === null &&
    d.comparison === undefined &&
    estimateOf(d.estimateText) !== 'invalid' &&
    !edited(formOf(server), inputOf(d.form, d.tagsText, d.estimateText));
  /**
   * 되살릴지 물을 입력을 고른다: 서버와 같은 것은 지우고 가장 새 것을 묻는다. `others`면 이 탭의 지금 입력은 빼고 본다
   * (고른 뒤 다음 것을 물을 때).
   */
  function offerDraft(server: TaskDetailView, others = false): void {
    const name = draftName;
    const tab = formDraftTabKey(name);
    let next: StoredFormDraft<TaskDraft> | undefined;
    for (const entry of listFormDrafts(name, parseTaskDraft)) {
      if (isServer(entry.value, server)) {
        removeFormDraftAt(entry.key);
        continue;
      }
      if (others && entry.key === tab) continue;
      next ??= entry;
    }
    if (next === undefined) {
      restorable = undefined;
      reserved = false;
      return;
    }
    const held = holdFormDraft(name, next);
    reserved = held.key === tab;
    restorable = held;
  }
  /** 되살리기: 폼·기준·판을 남길 때대로 되돌린다. 보내는 것은 없다(사용자가 "저장"을 누른다). */
  function restoreDraft(): void {
    const entry = restorable;
    const current = detailForm.task;
    if (entry === undefined || current === undefined || !canEdit || busy) return;
    const kept = entry.value;
    removeFormDraftAt(entry.key);
    restorable = undefined;
    reserved = false;
    detailForm.form = kept.form;
    detailForm.tagsText = kept.tagsText;
    detailForm.estimateText = kept.estimateText;
    detailForm.baseline = kept.baseline;
    detailForm.baseVersion = kept.baseVersion;
    detailForm.clashed = false;
    pending = undefined;
    comparison = undefined;
    preservedComparison = kept.comparison;
    error = undefined;
    parentChoice = '';
    childInitial = kept.child ?? undefined;
    childSeed += 1;
    resetLinkEditor();
    let comparing = false;
    if (kept.comparison !== undefined && estimateOf(kept.estimateText) !== 'invalid') {
      // Resume the paired fields/assignees/version from the 409, never a composed read with different relation timing.
      const reviewed = kept.comparison;
      const unchanged =
        kept.baseVersion === reviewed.baseVersion &&
        !edited(reviewed.base, kept.baseline) &&
        !edited(reviewed.mine, inputOf(kept.form, kept.tagsText, kept.estimateText));
      comparing = openComparison(
        { ...reviewed.server, version: reviewed.version },
        unchanged ? reviewed.choices : undefined,
      );
    } else if (estimateOf(kept.estimateText) === 'invalid') {
      error = m.tasks_estimate_invalid();
    } else if (current.version !== kept.baseVersion) {
      refresh(current);
    }
    offerDraft(current, true);
    if (!comparing) void focusLater('task-title');
  }
  function discardDraft(): void {
    const entry = restorable;
    if (entry === undefined) return;
    removeFormDraftAt(entry.key);
    restorable = undefined;
    reserved = false;
    if (detailForm.task !== undefined) offerDraft(detailForm.task, true);
  }
  $effect(() => {
    const mine = currentInput();
    const before = detailForm.baseline;
    const dirty =
      detailForm.changed || childDraft !== undefined || preservedComparison !== undefined;
    if (
      detailForm.task === undefined ||
      mine === undefined ||
      before === undefined ||
      detailForm.form === undefined
    )
      return;
    if (reserved || me.userId !== editingUser) return;
    if (dirty) {
      const kept = storeFormDraft(draftName, {
        baseVersion: detailForm.baseVersion,
        baseline: before,
        form: $state.snapshot(detailForm.form),
        tagsText: detailForm.tagsText,
        estimateText: detailForm.estimateText,
        child: childDraft ?? null,
        savedAt: new Date().toISOString(),
        comparison: comparison === undefined ? preservedComparison : $state.snapshot(comparison),
      } satisfies TaskDraft);
      taskDraftKept = kept;
    } else {
      clearFormDraft(draftName);
      taskDraftKept = true;
    }
  });
  // 본문 초안이 보류되어 위 효과가 일찍 끝나도 자식 입력의 보존 실패는 부모에 전달한다.
  $effect(() => {
    ondraftkept?.(
      taskDraftKept &&
        !linkChanged &&
        parentChoice === '' &&
        checklistDraftKept &&
        commentsDraftKept &&
        recurrenceDraftKept,
    );
  });
  // 폼이 사라지면(닫기·다른 업무 열기) 이 폼의 초안 상태도 끝난다: 다음 폼의 깨끗한 상태가 옛 실패를 물려받지 않게 한다.
  $effect(() => () => ondraftkept?.(true));
  // 사용자가 버리고 닫거나 다른 업무를 열어 이 폼이 없어지면 이 탭의 입력도 지운다(방금 버린 입력을 다음에 되살리자고 하지
  // 않게). 다른 계정으로 바뀌어 다시 읽는 중과 새로고침·탭 닫기는 여기를 거치지 않아 입력이 남는다.
  onDestroy(() => {
    if (!accountSwitching() && me.userId === editingUser) clearFormDraft(draftName);
  });

  /** 저장: 서버 기준 내용과 날짜가 다르면 먼저 후속 업무 영향을 미리 보고, 옮길 것이 있으면 사용자에게 묻는다. */
  const writeContext = (): ContextCapture => ({
    userId: me.userId,
    sequence: detailReadSequence,
    navigationGeneration: router.navigationGeneration,
    sessionGeneration: readTaskSessionGeneration(),
    safetyGeneration: accountSafety.generation,
    path: router.path,
  });
  function acceptsWrite(captured: ContextCapture): boolean {
    return (
      mounted &&
      !hiddenForAccount &&
      !hiddenForAccess &&
      !permissionBlocked &&
      !accountSafety.switching &&
      !accountSafety.restoring &&
      sameContext(captured, writeContext())
    );
  }
  function writeFailed(cause: unknown): void {
    if (cause instanceof ApiError) {
      if (cause.code === 'account_changed') accountBlocked = true;
      if ([403, 404].includes(cause.status) || cause.code === 'invalid_state')
        permissionBlocked = true;
      if (cause.status === 404) hiddenForAccess = true;
    }
    error = errorText(cause, m.tasks_save_failed());
  }
  async function submit(): Promise<void> {
    const input = currentInput();
    if (detailForm.task === undefined || input === undefined || busy || !canEdit) return;
    // 잘못된 예상 시간을 빈 값으로 보내면 저장된 값이 지워진다. 입력을 그대로 두고 알린다.
    if (estimateOf(detailForm.estimateText) === 'invalid') {
      error = m.tasks_estimate_invalid();
      return;
    }
    const before = detailForm.baseline ?? formOf(detailForm.task);
    const baseVersion = detailForm.baseVersion;
    const patch = changedTaskFields(before, $state.snapshot(input));
    busy = true;
    saving = true;
    error = undefined;
    const captured = writeContext();
    try {
      // 목록의 선행 정보는 오래됐을 수 있으므로 날짜가 바뀌면 언제나 서버에 묻는다.
      let undated: UndatedTaskView[] = [];
      if (input.start_date !== before.start_date || input.due_date !== before.due_date) {
        const plan = await work.previewSchedule(
          detailForm.task.id,
          input.start_date,
          input.due_date,
        );
        if (!acceptsWrite(captured)) return;
        if (plan.shifts.length > 0) {
          pending = {
            proposals: plan.shifts,
            undated: plan.undated,
            blocked: plan.blocked,
            input,
            patch,
            baseVersion,
            retried: false,
          };
          return;
        }
        undated = plan.undated;
      }
      await save(input, patch, baseVersion, [], undated);
    } catch (e: unknown) {
      if (e instanceof ApiError && e.code === 'account_changed') accountBlocked = true;
      if (acceptsWrite(captured)) writeFailed(e);
    } finally {
      if (captured.sequence === detailReadSequence) {
        busy = false;
        saving = false;
      }
    }
  }

  /** 미리보기 패널의 선택. 승인은 패널을 띄울 때 붙잡아 둔 입력과 기준 버전 그대로 보낸다. */
  async function decide(withShifts: boolean): Promise<void> {
    const p = pending;
    if (p === undefined || busy) return;
    busy = true;
    saving = true;
    error = undefined;
    const captured = writeContext();
    try {
      await save(p.input, p.patch, p.baseVersion, withShifts ? p.proposals : [], p.undated);
    } catch (e: unknown) {
      if (e instanceof ApiError && e.code === 'account_changed') accountBlocked = true;
      if (acceptsWrite(captured)) writeFailed(e);
    } finally {
      if (captured.sequence === detailReadSequence) {
        busy = false;
        saving = false;
      }
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
    patch: Partial<TaskInput>,
    baseVersion: number,
    shifts: ShiftProposalView[],
    undated: UndatedTaskView[] = [],
    /** 다시 계산해 보니 밀릴 업무가 없어 묻지 않고 저장했다(저장한 뒤 알린다, D-125). */
    noShiftNeeded = false,
  ): Promise<void> {
    if (detailForm.task === undefined) return;
    const captured = writeContext();
    const result = await work.updateTask(
      editingUser,
      detailForm.task.id,
      baseVersion,
      patch,
      shifts.map(toShift),
    );
    if (!acceptsWrite(captured)) return;
    if (result.kind === 'conflict') {
      pending = undefined;
      openComparison(result.current);
      return;
    }
    if (result.kind === 'shift_conflict') {
      // 아무것도 바뀌지 않았다. 지금 상태로 다시 계산해 다시 묻는다. 이제 옮길 것이 없으면 묻지 않고 그대로 저장하고
      // "밀릴 업무가 없어 그대로 저장"을 알린다(D-91 개정·D-125, R5 최종 A FA-2).
      const plan = await work.previewSchedule(detailForm.task.id, input.start_date, input.due_date);
      if (plan.shifts.length === 0) {
        pending = undefined;
        if (!acceptsWrite(captured)) return;
        await save(input, patch, baseVersion, [], plan.undated, true);
        return;
      }
      pending = {
        proposals: plan.shifts,
        undated: plan.undated,
        blocked: plan.blocked,
        input,
        patch,
        baseVersion,
        retried: true,
      };
      return;
    }
    fill(result.task);
    // 일반 완료는 토스트다(D-82). 날짜가 없어 옮기지 않은 업무 안내도 그 문구에 붙인다.
    notifyDone(
      [m.common_saved(), noShiftNeeded ? m.tasks_gantt_no_shifts() : '', undatedText(undated)]
        .filter((s) => s !== '')
        .join(' '),
      'task-save',
    );
    onchanged();
    await focusSave();
  }

  // 패널이 사라질 때 대화상자의 초점 가두기가 첫 입력란으로 초점을 옮기므로, 그 뒤 프레임에서 요소로 돌린다.
  async function focusLater(id: string): Promise<void> {
    const captured = writeContext();
    await tick();
    requestAnimationFrame(() => {
      if (acceptsWrite(captured)) document.getElementById(id)?.focus();
    });
  }

  async function focusSave(): Promise<void> {
    await focusLater('task-save');
  }

  async function reloadChecklist(next: Promise<unknown>): Promise<void> {
    const captured = writeContext();
    await next;
    if (detailForm.task === undefined || !acceptsWrite(captured)) return;
    const fresh = await work.task(detailForm.task.id);
    if (!acceptsWrite(captured)) return;
    if (detailForm.task.version === fresh.version) {
      detailForm.task = fresh;
    } else {
      refresh(fresh);
    }
    onchanged();
  }

  let latestRule = $state<TaskDetailView | undefined>();
  const recurrenceIdentity = $derived({ taskId, userId: me.userId });
  let mounted = true;
  onDestroy(() => {
    mounted = false;
  });
  async function saveRule(
    rule: RecurrenceRule | null,
    baseRule?: RecurrenceRule | null,
  ): Promise<void> {
    if (detailForm.task === undefined || hiddenForAccount) return;
    const identity = recurrenceIdentity;
    const id = detailForm.task.id;
    const stillCurrent = (): boolean =>
      mounted && identity === recurrenceIdentity && detailForm.task?.id === id;
    try {
      const base = latestRule ?? detailForm.task;
      const fresh = await work.setTaskRecurrence(
        editingUser,
        id,
        rule,
        base.series?.timezone ?? localTimeZone(),
        baseRule === undefined ? (base.series?.rule ?? null) : baseRule,
      );
      if (!stillCurrent()) return;
      latestRule = undefined;
      if (detailForm.task.version === fresh.version) detailForm.task = fresh;
      else refresh(fresh);
      onchanged();
    } catch (e: unknown) {
      if (stillCurrent()) {
        if (e instanceof ApiError && e.code === 'account_changed') accountBlocked = true;
        if (e instanceof TaskRuleConflict) latestRule = e.current;
      }
      throw e;
    }
  }

  /**
   * 선행 관계 지우기. 응답을 기다리는 동안은 다른 관계 저장을 막는다(늦게 온 지우기와 저장이 겹쳐 지운 관계를 다시
   * 만들지 않게). 고치던 관계를 지웠으면 편집 칸을 닫는다(그대로 두면 "관계 저장"이 지운 관계를 새로 만든다). 다른
   * 관계를 고치던 중이면 그 편집은 그대로 둔다. 실패하면 관계와 편집 내용을 그대로 두고 알린다.
   */
  async function removeLink(predecessor: string): Promise<void> {
    if (detailForm.task === undefined || busy) return;
    busy = true;
    error = undefined;
    try {
      const fresh = await work.removePredecessor(editingUser, detailForm.task.id, predecessor);
      if (detailForm.task.version === fresh.version) detailForm.task = fresh;
      else refresh(fresh);
      if (editing === predecessor) resetLinkEditor();
      notifyDone(m.tasks_link_removed(), 'task-link');
      onchanged();
      await focusLater('predecessors-heading');
    } catch (e: unknown) {
      if (e instanceof ApiError && e.code === 'account_changed') accountBlocked = true;
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
    const link = detailForm.task?.predecessors.find((l) => l.task_id === predecessor);
    if (link === undefined) return;
    editing = predecessor;
    linkKind = link.kind;
    lagText = String(link.lag_days);
    linkBase = { kind: link.kind, lag_days: link.lag_days };
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
    if (detailForm.task === undefined || busy || predecessor === '') return;
    // 고치던 관계가 그사이 없어졌으면(다시 읽은 판에 없음) 새로 만들지 않고 편집 칸을 닫는다.
    if (editing !== undefined && !detailForm.task.predecessors.some((l) => l.task_id === editing)) {
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
    const kind = linkKind;
    const base = linkBase;
    try {
      const plan = await work.previewLink(detailForm.task.id, predecessor, kind, lag);
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
      if (e instanceof ApiError && e.code === 'account_changed') accountBlocked = true;
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
    /** 다시 계산해 보니 밀릴 업무가 없어 묻지 않고 저장했다(저장한 뒤 알린다, D-125). */
    noShiftNeeded = false,
  ): Promise<void> {
    if (detailForm.task === undefined) return;
    const result = await work.setPredecessor(
      editingUser,
      detailForm.task.id,
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
      if (detailForm.task.version === current.version) detailForm.task = current;
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
      // 아무것도 바뀌지 않았다. 지금 상태로 다시 계산해 다시 묻는다. 이제 옮길 것이 없으면 묻지 않고 그대로 저장하고
      // "밀릴 업무가 없어 그대로 저장"을 알린다(D-91 개정·D-125, R5 최종 A FA-2).
      const plan = await work.previewLink(detailForm.task.id, predecessor, kind, lag);
      if (plan.shifts.length === 0) {
        linkPending = undefined;
        await saveLink(predecessor, kind, lag, base, [], plan.undated, true);
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
    if (detailForm.task.version === fresh.version) detailForm.task = fresh;
    else refresh(fresh);
    notifyDone(
      [m.tasks_link_saved(), noShiftNeeded ? m.tasks_gantt_no_shifts() : '', undatedText(undated)]
        .filter((s) => s !== '')
        .join(' '),
      'task-link',
    );
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
      if (e instanceof ApiError && e.code === 'account_changed') accountBlocked = true;
      error = errorText(e, m.tasks_predecessor_failed());
    } finally {
      busy = false;
    }
  }

  async function changeParent(id: string, parentId: string | null): Promise<void> {
    const choiceAtStart = parentChoice;
    error = undefined;
    try {
      const fresh = await work.setTaskParent(editingUser, id, parentId);
      if (parentChoice === choiceAtStart) parentChoice = '';
      if (detailForm.task?.version === fresh.version) detailForm.task = fresh;
      else refresh(fresh);
      notifyDone(m.tasks_parent_changed());
      onchanged();
    } catch (e: unknown) {
      if (e instanceof ApiError && e.code === 'account_changed') accountBlocked = true;
      error = errorText(e, m.tasks_parent_failed());
    }
  }
</script>

{#if error !== undefined}
  <p role="alert" class="text-sm text-destructive">{error}</p>
  {#if retryRead !== undefined}
    <Button
      variant="outline"
      onclick={() => {
        const retry = retryRead;
        retryRead = undefined;
        retry?.();
      }}>{m.common_retry()}</Button
    >
  {/if}
{/if}
{#if hiddenForAccount}
  <p role="alert">{m.tasks_account_changed()}</p>
  <Button onclick={() => void verifyAccount()}>{m.tasks_account_verify()}</Button>
{/if}
<div
  class="@container"
  hidden={hiddenForAccount || hiddenForAccess}
  inert={hiddenForAccount || hiddenForAccess}
>
  {#if detailForm.task === undefined && error === undefined}
    <!-- 읽는 동안 빈 창으로 두지 않는다. -->
    <p class="py-6 text-center text-sm text-muted-foreground" role="status">
      {m.modules_loading()}
    </p>
  {/if}
  {#if detailForm.task !== undefined && detailForm.form !== undefined}
    {@const current = detailForm.task}
    <!-- 비교 중의 값을 고정하고, 이미 보낸 자식 응답은 refresh에서 더 새 판만 채택한다. -->
    <div class="flex flex-col gap-4">
      <div class="flex flex-wrap items-baseline gap-2" inert={comparison !== undefined}>
        <span class="font-mono text-sm text-muted-foreground">{current.reference ?? ''}</span>
        {#if current.access === 'read' || readOnly}<span class="text-sm text-muted-foreground"
            >{m.tasks_read_only()}</span
          >{/if}
        <div class="ml-auto flex flex-wrap items-center gap-1">
          <MarkButtons kind="task" id={current.id} />
          {#if canEdit}
            <DeleteButton
              what={m.tasks_noun_task()}
              remove={() => work.trashTask(editingUser, current.id)}
              ondeleted={() => {
                onchanged();
                router.navigate(itemPath(current.workspace_id, current.item_id));
              }}
            />
          {/if}
        </div>
      </div>

      {#if restorable !== undefined}
        {@const kept = restorable.value}
        <div
          class="flex flex-wrap items-center gap-2 rounded border px-3 py-2 text-sm"
          role="region"
          aria-label={m.tasks_draft_region()}
          inert={comparison !== undefined}
          data-testid="task-draft-offer"
        >
          <span>{m.tasks_draft_found({ time: formatInstant(kept.savedAt) })}</span>
          {#if !canEdit}
            <span class="text-muted-foreground">{m.tasks_draft_read_only()}</span>
          {:else}
            <Button size="sm" variant="outline" disabled={busy} onclick={restoreDraft}
              >{m.tasks_draft_restore()}</Button
            >
          {/if}
          <Button size="sm" variant="ghost" onclick={discardDraft}>{m.tasks_draft_discard()}</Button
          >
        </div>
      {/if}

      {#if comparison !== undefined}
        <FieldConflict
          fields={conflictFields}
          base={comparison.base}
          mine={comparison.mine}
          server={comparison.server}
          bind:choices={comparison.choices}
          {busy}
          blocked={!canEdit}
          repeated={comparison.repeated}
          onsave={() => void applyChoices(true)}
          onedit={() => void applyChoices(false)}
          oncancel={() => {
            leaveComparison(false);
          }}
          onoriginal={edited(comparison.original, comparison.mine)
            ? () => {
                leaveComparison(true);
              }
            : undefined}
        />
      {/if}

      <div class="flex flex-col gap-4" inert={comparison !== undefined}>
        <!-- 제목 다음에 상태·우선순위·날짜를 두 칸으로 먼저 보이고(좁은 화면에서도 스크롤 없이), 설명은 그 아래에 둔다. -->
        <TaskFields
          bind:f={detailForm.form}
          bind:tagsText={detailForm.tagsText}
          bind:estimateText={detailForm.estimateText}
          previewing={pending !== undefined}
          {locked}
          {canEdit}
          {busy}
          {statusNames}
          {people}
          {peopleError}
          {peopleLabels}
          {toggleAssignee}
          {submit}
        />

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

        <TaskHierarchy
          {current}
          {canEdit}
          {subtasks}
          {parentCandidates}
          {byId}
          {label}
          {onopen}
          {changeParent}
          bind:parentChoice
          {childSeed}
          {childInitial}
          onchilddraft={(draft: { name: string; requestId: string | null } | undefined) => {
            childDraft = draft;
          }}
          createChild={async (requestId: string, title: string) => {
            await work.createTask(editingUser, current.item_id, requestId, title, current.id);
            onchanged();
          }}
        />

        <TaskDependencies
          {current}
          {canEdit}
          {busy}
          {byId}
          {candidates}
          {successors}
          {label}
          {onopen}
          {startEdit}
          {removeLink}
          {applyLink}
          {resetLinkEditor}
          {decideLink}
          {focusLater}
          bind:editing
          bind:predecessorChoice
          bind:linkKind
          bind:lagText
          bind:linkPending
        />

        <TaskRecurrence
          bind:this={recurrenceEditor}
          {current}
          {latestRule}
          {canEdit}
          {onopen}
          {saveRule}
          onunsaved={(value: boolean) => {
            recurrenceUnsaved = value;
          }}
          ondraftkept={(value: boolean) => {
            recurrenceDraftKept = value;
          }}
          onsaving={(value: boolean) => {
            recurrenceSaving = value;
          }}
        />

        <ChecklistForm
          preserveDraft={false}
          bind:this={checklistEditor}
          idPrefix={`task-${current.id}`}
          items={current.checklist.map((c) => ({ id: c.id, text: c.text, done: c.checked }))}
          {canEdit}
          onunsaved={(value: boolean) => {
            checklistUnsaved = value;
          }}
          ondraftkept={(value: boolean) => {
            checklistDraftKept = value;
          }}
          onadd={(entries: NewChecklistEntry[], user: string) =>
            reloadChecklist(
              work.addChecklist(
                user,
                current.id,
                entries.map((e) => ({ request_id: e.requestId, text: e.text, checked: e.done })),
              ),
            )}
          ontoggle={(id: string, done: boolean, user: string) =>
            reloadChecklist(work.updateChecklistItem(user, id, { checked: done }))}
          onrename={(id: string, text: string, user: string) =>
            reloadChecklist(work.updateChecklistItem(user, id, { text }))}
          onremove={(id: string, user: string) =>
            reloadChecklist(work.deleteChecklistItem(user, id))}
        />

        <TaskSidePanels
          archived={screenArchived}
          bind:this={commentsEditor}
          {current}
          {workspaces}
          canWrite={canEdit}
          onunsaved={(value: boolean) => {
            commentsUnsaved = value;
          }}
          ondraftkept={(value: boolean) => {
            commentsDraftKept = value;
          }}
        />
      </div>
    </div>
  {/if}
</div>
