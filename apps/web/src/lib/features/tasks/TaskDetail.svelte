<script lang="ts">
  import TaskHierarchy from './TaskHierarchy.svelte';
  import TaskFields from './TaskFields.svelte';
  import TaskDependencies from './TaskDependencies.svelte';
  import TaskRecurrence from './TaskRecurrence.svelte';
  import TaskSidePanels from './TaskSidePanels.svelte';
  import type { LinkPending } from './detail-state';
  import { parseTaskDraft, type TaskDraft } from './detail-draft';
  import { taskData as work } from '$lib/features/tasks/detail-data';
  import { taskScreenApi, composeScreenDetail, type DetailRead } from './screen-api';
  import {
    DiscardedTaskRead,
    sameContext,
    startCapturedRead,
    type CapturedRead,
    type ContextCapture,
  } from './context-read';
  import { readSessionGeneration } from '$lib/features/auth/session-signal';
  import {
    TASK_SESSION_RECOVERY,
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

  import ClashConfirm from './ClashConfirm.svelte';
  import { createTaskForm, type Merge } from './task-form.svelte';
  import { edited, estimateOf, formOf } from './form';
  import type { ClashRow } from './labels';
  import { assigneeLabels, clashRows, linkText, undatedText } from './labels';
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
  let conflict = $state<TaskDetailView | undefined>(undefined);
  let error = $state<string | undefined>(undefined);
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
  // 충돌 화면의 "내 입력으로 다시 저장"에서 나와 다른 사람이 서로 다르게 바꾼 칸(D-94). 보내기 전에 그 칸의 서버 값과
  // 내 값을 보이고 덮어쓸지 묻는다. 묻는 동안 업무 상세(충돌 알림 밖)는 잠가 보인 값 그대로 덮어쓰고, 승인하면 물을 때
  // 만든 병합(`merge`)을 그대로 저장한다. 취소하면 아무것도 보내지 않는다.
  interface ClashAsk {
    latest: TaskDetailView;
    merge: Merge;
    rows: ClashRow[];
  }
  let clashAsk = $state<ClashAsk | undefined>(undefined);
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
    clashAsk = undefined;
  }
  function applyMerge(merge: Merge): void {
    detailForm.applyMerge(merge);
    pending = undefined;
    clashAsk = undefined;
  }

  /**
   * 체크리스트·반복·선행 업무를 바꾼 뒤 서버의 새 판으로 다시 채운다. 고쳤지만 저장하지 않은 칸(태그·예상 시간은
   * 저장할 때 보내는 값으로 비교)은 지키고, 그 칸을 서버에서도 누가 바꿨으면 옛 판 번호를 남겨 저장할 때 충돌
   * 화면이 고르게 한다.
   */
  function refresh(fresh: TaskDetailView): void {
    const asking = clashAsk;
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
    if (asking !== undefined)
      askAgain(fresh.version > asking.latest.version ? fresh : asking.latest);
  }

  /**
   * 덮어쓰기 확인이 떠 있는 동안 먼저 보낸 체크리스트·반복·선행·상위 업무 요청이 돌아와 다시 채웠으면(`refresh`), 확인을
   * 닫지 않고 아는 가장 새 판(`latest`)으로 다시 묻는다(전에는 말없이 닫혀 초점이 문서로 빠졌다, 리뷰). 받은 판이 충돌
   * 판보다 새면 충돌 알림도 그 판을 보여 다시 저장이 그 판 번호로 간다. 더는 겹친 칸이 없으면 확인을 닫고 "내 입력으로
   * 다시 저장"으로 돌린다(입력은 그대로, 보내는 것은 없다).
   */
  function askAgain(latest: TaskDetailView): void {
    const mine = currentInput();
    if (mine === undefined || detailForm.baseline === undefined) return;
    if (conflict !== undefined && latest.version > conflict.version) conflict = latest;
    const merge = mergeOnto(latest, mine, detailForm.baseline);
    if (merge.clashes.length === 0) {
      void focusLater('task-save-mine');
      return;
    }
    clashAsk = {
      latest,
      merge,
      rows: clashRows(merge.clashes, merge.freshForm, mine, assigneeNames(latest)),
    };
  }

  // 충돌 확인에 담당자 이름을 보일 때 쓴다(지정할 수 있는 사람과, 지금 판·서버 판에 지정된 사람). 같은 이름은 구분한다.
  function assigneeNames(latest: TaskDetailView): Map<string, string> {
    return assigneeLabels([
      ...(detailForm.task?.assignees ?? []),
      ...latest.assignees,
      ...(people ?? []),
    ]);
  }
  // 폼의 담당자 이름표. 충돌 확인과 같은 글이라 확인에 보인 사람을 폼에서 찾을 수 있다.
  const peopleLabels = $derived(
    assigneeLabels([...(detailForm.task?.assignees ?? []), ...(people ?? [])]),
  );

  /**
   * 충돌 화면의 "내 입력으로 다시 저장"(D-94): 서버 최신 판(`latest`) 위에 내가 고친 칸만 올려 그 판 번호로 저장한다.
   * 내가 고치지 않은 칸은 최신 값을 따르고(전에는 폼 전체를 보내 그사이 다른 사람이 바꾼 제목 등을 옛 값으로
   * 되돌렸다), 내가 고친 칸 가운데 서버에서는 그대로이거나 같은 값이 된 칸은 내 값을 쓴다. 양쪽이 서로 다른 값으로
   * 바꾼 칸이 있으면 보내기 전에 그 칸의 서버 값과 내 값을 보이고 덮어쓸지 묻는다(전에는 묻지 않고 내 값으로 덮었다).
   */
  async function saveMine(latest: TaskDetailView): Promise<void> {
    const mine = currentInput();
    if (busy || mine === undefined || detailForm.baseline === undefined) return;
    // 잘못된 예상 시간은 병합 전에 막는다(입력을 그대로 두고 알린다).
    if (estimateOf(detailForm.estimateText) === 'invalid') {
      error = m.tasks_estimate_invalid();
      return;
    }
    const merge = mergeOnto(latest, mine, detailForm.baseline);
    if (merge.clashes.length > 0) {
      error = undefined;
      clashAsk = {
        latest,
        merge,
        rows: clashRows(merge.clashes, merge.freshForm, mine, assigneeNames(latest)),
      };
      return;
    }
    await saveMerged(latest, merge);
  }

  /**
   * 덮어쓰기 확인의 승인: 물을 때 만든 병합을 그대로 저장한다(묻는 동안 업무 상세는 잠겨 입력이 바뀌지 않는다). 양쪽이
   * 바꾼 단위는 병합에서 이미 내 값이고, 일정은 묶음 전체가 한쪽 값이라 보인 내 일정이 그대로 보내진다.
   */
  async function overwrite(): Promise<void> {
    const ask = clashAsk;
    if (ask === undefined || busy) return;
    await saveMerged(ask.latest, ask.merge);
  }

  /**
   * 병합한 입력을 최신 판 번호로 저장한다(서버가 그 판으로 검사한다). 최신 판이 다음 비교의 기준이 되므로, 그사이 또
   * 바뀌어 다시 409가 오면(충돌 화면이 더 새 판을 보인다) 다음 "내 입력으로 다시 저장"은 방금 받은 값을 고치지 않은
   * 칸으로 보고 더 새 판 위에 다시 올린다. 실패해도 입력은 폼에 그대로 있다.
   */
  async function saveMerged(latest: TaskDetailView, merge: Merge): Promise<void> {
    applyMerge(merge);
    detailForm.task = latest;
    detailForm.baseline = formOf(latest);
    detailForm.baseVersion = latest.version;
    detailForm.clashed = false;
    await submit(latest);
    // 충돌 화면이 남으면(저장 실패. 다시 409는 `save`가 돌린다) 누른 단추가 사라졌거나 잠겼던 자리이므로 "내 입력으로
    // 다시 저장"으로 돌린다. 미리보기 패널이 떴으면 초점은 패널에 있다.
    if (conflict !== undefined && pending === undefined) await focusLater('task-save-mine');
  }

  let detailReadSequence = 0;
  $effect(() => {
    const id = taskId;
    const navigation = untrack(() => aggregate) ? router.navigationGeneration : undefined;
    const parentItemId = untrack(() => itemId);
    const sequence = ++detailReadSequence;
    const capture = (): ContextCapture => ({
      userId: me.userId,
      sequence: detailReadSequence,
      navigationGeneration: router.navigationGeneration,
      sessionGeneration: readSessionGeneration(),
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
    conflict = undefined;
    clashAsk = undefined;
    error = undefined;
    screenArchived = undefined;
    let aggregateRead: DetailRead | undefined;
    const read = untrack(() => (aggregate ? initialRead?.(id)?.take() : undefined));
    const pending = aggregate
      ? (read === undefined
          ? startCapturedRead(snapshot, capture, taskScreenApi.detail(id))
              .take()
              .then((value) => ({ value, captured: snapshot }))
          : readTaskWithRecovery(
              snapshot,
              capture,
              active,
              sessionRecovery,
              () => taskScreenApi.detail(id),
              read,
            )
        ).then(({ value, captured }) => {
          snapshot = captured;
          aggregateRead = value;
          return composeScreenDetail(value);
        })
      : work.task(id, parentItemId);
    void pending
      .then((detail) => {
        if (!isCurrent()) return;
        // 되살릴지 물을 입력은 채우기 전에 고른다(이 탭의 키에 남은 것을 입력 보존 효과가 지우기 전에 옮긴다).
        offerDraft(detail);
        fill(detail);
        peopleError = undefined;
        if (aggregateRead !== undefined) {
          screenArchived = aggregateRead.archived;
          definitions = [];
          people = undefined;
          void startCapturedRead(snapshot, capture, taskScreenApi.options(detail.item_id))
            .take()
            .then((options) => {
              if (!isCurrent()) return;
              if (options.statuses.state === 'ready') definitions = options.statuses.value;
              else error = m.tasks_load_failed();
              if (options.assignable.state === 'ready') people = options.assignable.value;
              else peopleError = m.tasks_assignable_load_failed();
            })
            .catch((cause: unknown) => {
              if (!isCurrent() || cause instanceof DiscardedTaskRead) return;
              if (cause instanceof ApiError && [401, 403, 404].includes(cause.status)) {
                detailForm.task = undefined;
                error = errorText(cause, m.tasks_task_load_failed());
              } else {
                error = m.tasks_load_failed();
                peopleError = m.tasks_assignable_load_failed();
              }
            });
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
      })
      .catch((e: unknown) => {
        if (isCurrent() && !(e instanceof DiscardedTaskRead))
          error =
            e instanceof ApiError && e.status === 404
              ? m.tasks_not_found()
              : errorText(e, m.tasks_task_load_failed());
      });
    return () => {
      cancelled = true;
    };
  });

  const canEdit = $derived(
    !hiddenForAccount &&
      !readOnly &&
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
    if (entry === undefined || current === undefined || current.access === 'read' || busy) return;
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
    conflict = undefined;
    pending = undefined;
    clashAsk = undefined;
    error = undefined;
    parentChoice = '';
    childInitial = kept.child ?? undefined;
    childSeed += 1;
    resetLinkEditor();
    // 남길 때의 판보다 서버가 앞섰으면 기존 다시 채우기로 병합한다(양쪽이 다르게 바꾼 칸은 옛 판 번호를 남겨 저장이 충돌
    // 화면으로 간다). 같은 판이면 그 판으로 저장한다.
    if (current.version !== kept.baseVersion) refresh(current);
    else detailForm.task = current;
    offerDraft(current, true);
    void focusLater('task-title');
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
    const dirty = detailForm.changed || childDraft !== undefined;
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
  async function submit(base: TaskDetailView): Promise<void> {
    const input = currentInput();
    if (detailForm.task === undefined || input === undefined || busy) return;
    // 잘못된 예상 시간을 빈 값으로 보내면 저장된 값이 지워진다. 입력을 그대로 두고 알린다.
    if (estimateOf(detailForm.estimateText) === 'invalid') {
      error = m.tasks_estimate_invalid();
      return;
    }
    busy = true;
    saving = true;
    error = undefined;
    try {
      // 목록의 선행 정보는 오래됐을 수 있으므로 날짜가 바뀌면 언제나 서버에 묻는다.
      let undated: UndatedTaskView[] = [];
      if (input.start_date !== base.start_date || input.due_date !== base.due_date) {
        const plan = await work.previewSchedule(
          detailForm.task.id,
          input.start_date,
          input.due_date,
        );
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
      if (e instanceof ApiError && e.code === 'account_changed') accountBlocked = true;
      error = errorText(e, m.tasks_save_failed());
    } finally {
      busy = false;
      saving = false;
    }
  }

  /** 미리보기 패널의 선택. 승인은 패널을 띄울 때 붙잡아 둔 입력과 기준 버전 그대로 보낸다. */
  async function decide(withShifts: boolean): Promise<void> {
    const p = pending;
    if (p === undefined || busy) return;
    busy = true;
    saving = true;
    error = undefined;
    try {
      await save(p.input, p.baseVersion, withShifts ? p.proposals : [], p.undated);
    } catch (e: unknown) {
      if (e instanceof ApiError && e.code === 'account_changed') accountBlocked = true;
      error = errorText(e, m.tasks_save_failed());
    } finally {
      busy = false;
      saving = false;
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
    /** 다시 계산해 보니 밀릴 업무가 없어 묻지 않고 저장했다(저장한 뒤 알린다, D-125). */
    noShiftNeeded = false,
    // 후속 업무 미리보기에서 승인한 저장(덮어쓰기 뒤 미리보기 승인 등)이 다시 409면, 패널과 함께 누른 단추가 없어져
    // 초점이 문서로 빠지므로 "내 입력으로 다시 저장"으로 돌린다. 폼의 "저장"은 그대로 있으므로 두고(2차 검증 nit),
    // "내 입력으로 다시 저장"·덮어쓰기에서 바로 보낸 저장은 `saveMerged`가 돌린다. 미리보기를 비운 뒤의 재시도는
    // 처음 저장의 값을 넘겨받는다(RA-2).
    refocus = pending !== undefined,
  ): Promise<void> {
    if (detailForm.task === undefined) return;
    const result = await work.updateTask(
      editingUser,
      detailForm.task.id,
      baseVersion,
      input,
      shifts.map(toShift),
    );
    if (result.kind === 'conflict') {
      pending = undefined;
      conflict = result.current;
      if (refocus) await focusLater('task-save-mine');
      return;
    }
    if (result.kind === 'shift_conflict') {
      // 아무것도 바뀌지 않았다. 지금 상태로 다시 계산해 다시 묻는다. 이제 옮길 것이 없으면 묻지 않고 그대로 저장하고
      // "밀릴 업무가 없어 그대로 저장"을 알린다(D-91 개정·D-125, R5 최종 A FA-2).
      const plan = await work.previewSchedule(detailForm.task.id, input.start_date, input.due_date);
      if (plan.shifts.length === 0) {
        pending = undefined;
        await save(input, baseVersion, [], plan.undated, true, refocus);
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
    if (detailForm.task === undefined) return;
    const fresh = await work.task(detailForm.task.id);
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
{/if}
{#if hiddenForAccount}
  <p role="alert">{m.tasks_account_changed()}</p>
  <Button onclick={() => void verifyAccount()}>{m.tasks_account_verify()}</Button>
{/if}
<div class="@container" hidden={hiddenForAccount} inert={hiddenForAccount}>
  {#if detailForm.task === undefined && error === undefined}
    <!-- 읽는 동안 빈 창으로 두지 않는다. -->
    <p class="py-6 text-center text-sm text-muted-foreground" role="status">
      {m.modules_loading()}
    </p>
  {/if}
  {#if detailForm.task !== undefined && detailForm.form !== undefined}
    {@const current = detailForm.task}
    <!-- 덮어쓰기 확인이 떠 있는 동안에는 충돌 알림(확인 포함) 말고는 모두 잠근다. 체크리스트·반복·선행·상위 업무를
       바꾸면 다시 채우기가 확인을 말없이 닫고, 보인 값과 저장할 값이 어긋날 수 있다. -->
    <div class="flex flex-col gap-4">
      <div class="flex flex-wrap items-baseline gap-2" inert={clashAsk !== undefined}>
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
          inert={clashAsk !== undefined}
          data-testid="task-draft-offer"
        >
          <span>{m.tasks_draft_found({ time: formatInstant(kept.savedAt) })}</span>
          {#if current.access === 'read'}
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

      {#if conflict !== undefined}
        {@const latest = conflict}
        <!-- 알림은 문장에만 둔다(안의 덮어쓰기 확인이 따로 알리므로 알림 전체를 두 번 읽지 않게). -->
        <div class="rounded-md border border-amber-500/50 p-3 text-sm">
          <p role="alert">
            {m.tasks_conflict({ title: latest.title, status: latest.status })}
          </p>
          <div class="mt-2 flex gap-2">
            <Button
              size="sm"
              variant="outline"
              onclick={() => {
                fill(latest);
                conflict = undefined;
                // 누른 단추가 알림과 함께 사라지므로 다시 채운 폼의 첫 칸으로 돌린다(전에는 문서로 빠졌다).
                void focusLater('task-title');
              }}>{m.tasks_load_server()}</Button
            >
            <Button
              id="task-save-mine"
              size="sm"
              disabled={busy || clashAsk !== undefined}
              onclick={() => void saveMine(latest)}>{m.tasks_save_mine()}</Button
            >
          </div>
          {#if clashAsk !== undefined}
            <ClashConfirm
              rows={clashAsk.rows}
              {busy}
              onconfirm={() => void overwrite()}
              oncancel={() => {
                clashAsk = undefined;
                void focusLater('task-save-mine');
              }}
            />
          {/if}
        </div>
      {/if}

      <div class="flex flex-col gap-4" inert={clashAsk !== undefined}>
        <!-- 제목 다음에 상태·우선순위·날짜를 두 칸으로 먼저 보이고(좁은 화면에서도 스크롤 없이), 설명은 그 아래에 둔다. -->
        <TaskFields
          bind:f={detailForm.form}
          bind:tagsText={detailForm.tagsText}
          bind:estimateText={detailForm.estimateText}
          {current}
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
