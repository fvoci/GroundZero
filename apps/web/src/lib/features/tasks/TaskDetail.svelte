<script lang="ts">
  // 업무 상세 (D-31): 필드·담당자·하위 업무·체크리스트·역방향 링크.
  // 필드 저장은 기준 버전으로 충돌을 막는다. 충돌하면 입력을 지우지 않고 서버 내용과 함께 보여 준다(D-53과 같은 원칙).
  import { onDestroy, tick, untrack } from 'svelte';
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
    TaskPriority,
    TaskStatus,
    TaskView,
    PlanBlockedView,
    UndatedTaskView,
  } from '$lib/api/client';
  import {
    api,
    ApiError,
    TaskRuleConflict,
    LINK_KINDS,
    MAX_LAG_DAYS,
    TASK_PRIORITIES,
  } from '$lib/api/client';
  import { accountSwitching, notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { errorText } from '$lib/api/errors';
  import type { NewChecklistEntry } from '$lib/features/checklists/checklist-item';
  import type { StoredFormDraft } from '$lib/features/common/form-draft';
  import {
    clearFormDraft,
    formDraftTabKey,
    holdFormDraft,
    listFormDrafts,
    removeFormDraftAt,
    storeFormDraft,
  } from '$lib/features/common/form-draft';
  import { canonicalJson } from '$lib/features/documents/drafts';
  import { formatInstant } from '$lib/display';
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
  import { Textarea } from '$lib/ui/textarea';
  import * as Select from '$lib/ui/select';

  import ClashConfirm from './ClashConfirm.svelte';
  import type { MergeUnit } from './form';
  import {
    edited,
    estimateOf,
    formOf,
    mergeEstimate,
    mergeForm,
    scheduleOf,
    trimLikeServer,
  } from './form';
  import type { ClashRow } from './labels';
  import {
    LINK_KIND_LABEL,
    LINK_KIND_MEANING,
    PRIORITY_LABEL,
    assigneeLabels,
    clashRows,
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

  const { taskId, tasks, workspaces, onchanged, onopen, onunsaved, onsaving, ondraftkept }: Props =
    $props();

  const editingUser = untrack(() => me.userId);
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
  $effect(() => {
    const itemId = task?.item_id;
    if (itemId === undefined) return;
    let cancelled = false;
    void api
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
  let childDraft = $state<{ name: string; requestId: string | null } | undefined>();
  let checklistUnsaved = $state(false);
  let commentsUnsaved = $state(false);
  let recurrenceUnsaved = $state(false);
  let checklistDraftKept = $state(true);
  let commentsDraftKept = $state(true);
  let recurrenceDraftKept = $state(true);
  let taskDraftKept = $state(true);
  const parentCandidates = $derived(
    tasks.filter((t) => t.id !== taskId && t.parent_id === null && t.id !== task?.parent_id),
  );
  const candidates = $derived(
    task === undefined
      ? []
      : tasks.filter(
          (t) => t.id !== taskId && !(task?.predecessors.some((p) => p.task_id === t.id) ?? false),
        ),
  );

  // 다시 채울 때 비교할 기준(마지막으로 다 불러온 판의 값, 사용자가 바꾸지 않은 칸만 새 값으로 옮긴다)과, 같은 칸을
  // 양쪽에서 바꿔 충돌이 있었는지(다음 저장·불러오기까지 지킨다), 그때 남겨 둘 옛 판 번호. 기준은 저장하지 않은 입력이
  // 있는지(`unsaved`)도 정하므로 상태로 둔다.
  let baseline = $state.raw<TaskInput | undefined>(undefined);
  let baseVersion = 0;
  let clashed = false;

  function fill(next: TaskDetailView): void {
    task = next;
    form = formOf(next);
    baseline = formOf(next);
    baseVersion = next.version;
    clashed = false;
    pending = undefined;
    clashAsk = undefined;
    tagsText = next.tags.join(', ');
    estimateText = next.estimate_minutes === null ? '' : String(next.estimate_minutes);
  }

  /** 지금 입력을 기준 판에서 서버의 새 판으로 옮긴 결과(`mergeOnto`). */
  interface Merge {
    freshForm: TaskInput;
    form: TaskInput;
    baseline: TaskInput;
    /** 나와 서버가 서로 다른 값으로 바꾼 단위(일정은 시작일·마감일·마일스톤 한 묶음). */
    clashes: MergeUnit[];
    keepTagsText: boolean;
    keepEstimateText: boolean;
  }

  /**
   * 내 입력(`mine`)을 기준(`before`)에서 서버의 새 판(`fresh`)으로 옮긴다. 고치지 않은 칸은 새 값, 고친 칸은 내 값이고,
   * 고친 칸을 서버에서도 다른 값으로 바꿨으면 `clashes`에 든다. 태그는 저장할 때 보내는 값으로 견주고, 예상 시간은
   * 입력한 글자의 값으로 따로 맞춘다(병합에는 고치지 않은 칸으로 넘긴다). 다시 채우기와 충돌 뒤 저장이 같이 쓴다.
   */
  function mergeOnto(fresh: TaskDetailView, mine: TaskInput, before: TaskInput): Merge {
    const freshForm = formOf(fresh);
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
    return {
      freshForm,
      form: merged.form,
      baseline: { ...merged.baseline, estimate_minutes: estimate.baseline },
      clashes: estimate.clash ? [...merged.clashes, 'estimate_minutes'] : merged.clashes,
      keepTagsText: canonicalJson(mine.tags) !== canonicalJson(before.tags),
      keepEstimateText: estimate.keepText,
    };
  }

  /** 병합한 값을 폼에 옮긴다. 태그·예상 시간 글자는 내가 고치지 않았을 때만 새 값으로 바꾼다. */
  function applyMerge(merge: Merge): void {
    form = merge.form;
    pending = undefined;
    clashAsk = undefined;
    if (!merge.keepTagsText) tagsText = merge.freshForm.tags.join(', ');
    if (!merge.keepEstimateText) {
      const fresh = merge.freshForm.estimate_minutes;
      estimateText = fresh === null ? '' : String(fresh);
    }
  }

  /**
   * 체크리스트·반복·선행 업무를 바꾼 뒤 서버의 새 판으로 다시 채운다. 고쳤지만 저장하지 않은 칸(태그·예상 시간은
   * 저장할 때 보내는 값으로 비교)은 지키고, 그 칸을 서버에서도 누가 바꿨으면 옛 판 번호를 남겨 저장할 때 충돌
   * 화면이 고르게 한다.
   */
  function refresh(fresh: TaskDetailView): void {
    const asking = clashAsk;
    const mine = currentInput();
    if (task === undefined || mine === undefined || baseline === undefined) {
      fill(fresh);
      return;
    }
    const merge = mergeOnto(fresh, mine, baseline);
    baseline = merge.baseline;
    // 충돌은 지금의 병합으로 다시 본다. 기준은 사용자가 바꾼 칸을 옛 값으로 지키므로, 서버가 그 칸을 다르게 두는 한 계속
    // 충돌이고, 사용자가 서버 값으로 맞추면 풀려 최신 판으로 저장한다.
    clashed = merge.clashes.length > 0;
    if (!clashed) baseVersion = fresh.version;
    task = clashed ? { ...fresh, version: baseVersion } : fresh;
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
    if (mine === undefined || baseline === undefined) return;
    if (conflict !== undefined && latest.version > conflict.version) conflict = latest;
    const merge = mergeOnto(latest, mine, baseline);
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
    return assigneeLabels([...(task?.assignees ?? []), ...latest.assignees, ...(people ?? [])]);
  }
  // 폼의 담당자 이름표. 충돌 확인과 같은 글이라 확인에 보인 사람을 폼에서 찾을 수 있다.
  const peopleLabels = $derived(assigneeLabels([...(task?.assignees ?? []), ...(people ?? [])]));

  /**
   * 충돌 화면의 "내 입력으로 다시 저장"(D-94): 서버 최신 판(`latest`) 위에 내가 고친 칸만 올려 그 판 번호로 저장한다.
   * 내가 고치지 않은 칸은 최신 값을 따르고(전에는 폼 전체를 보내 그사이 다른 사람이 바꾼 제목 등을 옛 값으로
   * 되돌렸다), 내가 고친 칸 가운데 서버에서는 그대로이거나 같은 값이 된 칸은 내 값을 쓴다. 양쪽이 서로 다른 값으로
   * 바꾼 칸이 있으면 보내기 전에 그 칸의 서버 값과 내 값을 보이고 덮어쓸지 묻는다(전에는 묻지 않고 내 값으로 덮었다).
   */
  async function saveMine(latest: TaskDetailView): Promise<void> {
    const mine = currentInput();
    if (busy || mine === undefined || baseline === undefined) return;
    // 잘못된 예상 시간은 병합 전에 막는다(입력을 그대로 두고 알린다).
    if (estimateOf(estimateText) === 'invalid') {
      error = m.tasks_estimate_invalid();
      return;
    }
    const merge = mergeOnto(latest, mine, baseline);
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
    task = latest;
    baseline = formOf(latest);
    baseVersion = latest.version;
    clashed = false;
    await submit(latest);
    // 충돌 화면이 남으면(저장 실패. 다시 409는 `save`가 돌린다) 누른 단추가 사라졌거나 잠겼던 자리이므로 "내 입력으로
    // 다시 저장"으로 돌린다. 미리보기 패널이 떴으면 초점은 패널에 있다.
    if (conflict !== undefined && pending === undefined) await focusLater('task-save-mine');
  }

  $effect(() => {
    const id = taskId;
    let cancelled = false;
    task = undefined;
    latestRule = undefined;
    conflict = undefined;
    clashAsk = undefined;
    error = undefined;
    void api
      .task(id)
      .then((detail) => {
        if (cancelled) return;
        // 되살릴지 물을 입력은 채우기 전에 고른다(이 탭의 키에 남은 것을 입력 보존 효과가 지우기 전에 옮긴다).
        offerDraft(detail);
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

  const canEdit = $derived(!hiddenForAccount && task !== undefined && task.access !== 'read');
  // 폼을 저장하는 동안은 폼 칸을 잠근다: 저장되면 폼을 저장된 판으로 다시 채우므로(`fill`) 그사이 친 글이 말없이
  // 사라졌다. 관계·상위 업무 요청은 돌아오면 고친 칸을 지켜 다시 채우므로(`refresh`) 잠그지 않는다.
  let saving = $state(false);
  const locked = $derived(!canEdit || saving);

  /** 폼 값·태그 글자·예상 시간 글자를 저장될 모양의 입력으로(지금 입력과 보관한 입력을 같은 규칙으로 견준다). */
  function inputOf(f: TaskInput, tags: string, estimate: string): TaskInput {
    const minutes = estimateOf(estimate);
    return {
      ...f,
      // 병합과 같은 규칙(마일스톤이면 시작일 없음, 빈 칸은 없음)이라 병합이 고른 일정이 그대로 보내진다.
      ...scheduleOf(f),
      // 잘못된 입력은 저장 전에 막는다(submit). 여기서는 비교용으로만 쓴다.
      estimate_minutes: minutes === 'invalid' ? null : minutes,
      // 서버(`validate_tags`)처럼 앞뒤 공백을 떼고 겹친 태그는 처음 것만 둔다: 비교·확인·보내는 값이 저장될 값과 같다.
      tags: [
        ...new Set(
          tags
            .split(',')
            .map(trimLikeServer)
            .filter((t) => t !== ''),
        ),
      ],
    };
  }

  function currentInput(): TaskInput | undefined {
    return form === undefined ? undefined : inputOf(form, tagsText, estimateText);
  }

  // 기준과 다르게 고친 칸이 있는지. 병합과 같은 판정이다: 저장될 모양으로 견주고(제목·설명은 서버처럼 앞뒤 공백을 떼고
  // 태그는 서버처럼 정리), 예상 시간은 입력한 글자의 값으로 본다(잘못된 글자는 고친 것이다). 편집할 수 있는지와 무관하다
  // (계정이 바뀌어 폼을 숨겨도 고친 입력은 고친 입력이라 브라우저에 남긴다).
  const changed = $derived.by(() => {
    const mine = currentInput();
    if (mine === undefined || baseline === undefined) return false;
    return (
      estimateOf(estimateText) !== baseline.estimate_minutes ||
      edited(baseline, { ...mine, estimate_minutes: baseline.estimate_minutes })
    );
  });
  // 업무 대화상자가 닫기 전에 버릴지 물을 입력이 있는지(편집할 수 있을 때만).
  const unsaved = $derived(
    !hiddenForAccount &&
      (changed ||
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
  // 재전송 없음). 관계·상위 선택·하위 업무 제목도 이 초안과 이탈 확인에 포함한다.
  /** 선행 관계 편집 칸의 입력(D-128 대표 경로의 "선행 지정"). `editing`은 고치던 관계의 선행 업무, 새 관계면 null. */
  interface LinkDraft {
    editing: string | null;
    predecessor: string;
    kind: LinkKind;
    lag: string;
    base: LinkBase | null;
  }
  interface TaskDraft {
    baseVersion: number;
    baseline: TaskInput;
    form: TaskInput;
    tagsText: string;
    estimateText: string;
    /** 관계 편집 칸에 입력이 있을 때만. 옛 초안에는 없다(null). */
    link: LinkDraft | null;
    parent: string;
    child: { name: string; requestId: string | null } | null;
    savedAt: string;
  }
  const draftName = $derived(`task:${taskId}`);
  const isLinkKind = (value: unknown): value is LinkKind => LINK_KINDS.some((k) => k === value);
  function parseLinkDraft(value: unknown): LinkDraft | null | undefined {
    if (value === null || value === undefined) return null;
    if (typeof value !== 'object') return undefined;
    if (!(
      'editing' in value &&
      'predecessor' in value &&
      'kind' in value &&
      'lag' in value &&
      'base' in value
    ))
      return undefined;
    const { editing: edited, predecessor, kind, lag, base } = value;
    if (
      (edited !== null && typeof edited !== 'string') ||
      typeof predecessor !== 'string' ||
      !isLinkKind(kind) ||
      typeof lag !== 'string'
    )
      return undefined;
    if (base !== null) {
      if (typeof base !== 'object' || !('kind' in base && 'lag_days' in base)) return undefined;
      if (!isLinkKind(base.kind) || typeof base.lag_days !== 'number') return undefined;
      return {
        editing: edited,
        predecessor,
        kind,
        lag,
        base: { kind: base.kind, lag_days: base.lag_days },
      };
    }
    return { editing: edited, predecessor, kind, lag, base: null };
  }
  function isTaskInput(value: unknown): value is TaskInput {
    if (typeof value !== 'object' || value === null) return false;
    if (!(
      'title' in value &&
      'description' in value &&
      'status' in value &&
      'priority' in value &&
      'start_date' in value &&
      'due_date' in value &&
      'estimate_minutes' in value &&
      'tags' in value &&
      'assignee_ids' in value &&
      'milestone' in value
    ))
      return false;
    const text = (x: unknown): x is string => typeof x === 'string';
    const dateOrNull = (x: unknown): x is string | null => x === null || typeof x === 'string';
    const texts = (x: unknown): x is string[] => Array.isArray(x) && x.every(text);
    return (
      text(value.title) &&
      text(value.description) &&
      text(value.status) &&
      TASK_PRIORITIES.some((p) => p === value.priority) &&
      dateOrNull(value.start_date) &&
      dateOrNull(value.due_date) &&
      (value.estimate_minutes === null || typeof value.estimate_minutes === 'number') &&
      texts(value.tags) &&
      texts(value.assignee_ids) &&
      typeof value.milestone === 'boolean'
    );
  }
  function parseTaskDraft(value: unknown): TaskDraft | undefined {
    if (typeof value !== 'object' || value === null) return undefined;
    if (!(
      'baseVersion' in value &&
      'baseline' in value &&
      'form' in value &&
      'tagsText' in value &&
      'estimateText' in value &&
      'savedAt' in value
    ))
      return undefined;
    const { baseVersion: version, baseline: before, form: kept, savedAt } = value;
    const { tagsText: tags, estimateText: estimate } = value;
    const link = parseLinkDraft('link' in value ? value.link : null);
    if (
      typeof version !== 'number' ||
      typeof tags !== 'string' ||
      typeof estimate !== 'string' ||
      typeof savedAt !== 'string' ||
      !isTaskInput(before) ||
      !isTaskInput(kept) ||
      link === undefined
    )
      return undefined;
    return {
      baseVersion: version,
      baseline: before,
      form: kept,
      tagsText: tags,
      estimateText: estimate,
      link,
      parent: 'parent' in value && typeof value.parent === 'string' ? value.parent : '',
      child:
        'child' in value &&
        typeof value.child === 'object' &&
        value.child !== null &&
        'name' in value.child &&
        typeof value.child.name === 'string' &&
        'requestId' in value.child &&
        (value.child.requestId === null || typeof value.child.requestId === 'string')
          ? { name: value.child.name, requestId: value.child.requestId }
          : null,
      savedAt,
    };
  }
  let restorable = $state<StoredFormDraft<TaskDraft> | undefined>(undefined);
  let childInitial = $state<{ name: string; requestId: string | null } | undefined>();
  let childSeed = $state(0);
  // 묻는 입력이 보류 키로 옮기지 못해 이 탭의 키에 그대로 있으면(저장소 가득 참) 고를 때까지 그 키에 새 입력을 쓰지 않는다.
  let reserved = false;
  /** 서버 내용과 같은(이미 반영된) 입력인지. 관계 편집 칸에 입력이 남아 있으면 같지 않다. */
  const isServer = (d: TaskDraft, server: TaskDetailView): boolean =>
    d.link === null &&
    d.parent === '' &&
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
    const current = task;
    if (entry === undefined || current === undefined || current.access === 'read' || busy) return;
    const kept = entry.value;
    removeFormDraftAt(entry.key);
    restorable = undefined;
    reserved = false;
    form = kept.form;
    tagsText = kept.tagsText;
    estimateText = kept.estimateText;
    baseline = kept.baseline;
    baseVersion = kept.baseVersion;
    clashed = false;
    conflict = undefined;
    pending = undefined;
    clashAsk = undefined;
    error = undefined;
    parentChoice = kept.parent;
    childInitial = kept.child ?? undefined;
    childSeed += 1;
    // 관계 편집 칸: 고치던 관계가 아직 있으면 그 편집(기준 관계 포함)을, 없어졌으면 새 관계 입력으로 되살린다. 기준 관계가
    // 그사이 바뀌었으면 "관계 저장"이 서버의 409로 알고 최신 관계를 보인다(말없이 덮지 않음).
    const link = kept.link;
    if (link === null) resetLinkEditor();
    else {
      const stillLinked =
        link.editing !== null && current.predecessors.some((l) => l.task_id === link.editing);
      editing = stillLinked && link.editing !== null ? link.editing : undefined;
      predecessorChoice =
        stillLinked || !candidates.some((c) => c.id === link.predecessor) ? '' : link.predecessor;
      linkKind = link.kind;
      lagText = link.lag;
      linkBase = stillLinked ? link.base : null;
    }
    // 남길 때의 판보다 서버가 앞섰으면 기존 다시 채우기로 병합한다(양쪽이 다르게 바꾼 칸은 옛 판 번호를 남겨 저장이 충돌
    // 화면으로 간다). 같은 판이면 그 판으로 저장한다.
    if (current.version !== kept.baseVersion) refresh(current);
    else task = current;
    offerDraft(current, true);
    void focusLater('task-title');
  }
  function discardDraft(): void {
    const entry = restorable;
    if (entry === undefined) return;
    removeFormDraftAt(entry.key);
    restorable = undefined;
    reserved = false;
    if (task !== undefined) offerDraft(task, true);
  }
  $effect(() => {
    const mine = currentInput();
    const before = baseline;
    const dirty = changed || linkChanged || parentChoice !== '' || childDraft !== undefined;
    if (task === undefined || mine === undefined || before === undefined || form === undefined)
      return;
    if (reserved || me.userId !== editingUser) return;
    if (dirty) {
      const kept = storeFormDraft(draftName, {
        baseVersion,
        baseline: before,
        form: $state.snapshot(form),
        tagsText,
        estimateText,
        parent: parentChoice,
        child: childDraft ?? null,
        link: linkChanged
          ? {
              editing: editing ?? null,
              predecessor: predecessorChoice,
              kind: linkKind,
              lag: lagText,
              base: linkBase === null ? null : { kind: linkBase.kind, lag_days: linkBase.lag_days },
            }
          : null,
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
    ondraftkept?.(taskDraftKept && checklistDraftKept && commentsDraftKept && recurrenceDraftKept);
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
    if (task === undefined || input === undefined || busy) return;
    // 잘못된 예상 시간을 빈 값으로 보내면 저장된 값이 지워진다. 입력을 그대로 두고 알린다.
    if (estimateOf(estimateText) === 'invalid') {
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
  ): Promise<void> {
    if (task === undefined) return;
    // 후속 업무 미리보기에서 승인한 저장(덮어쓰기 뒤 미리보기 승인 등)이 다시 409면, 패널과 함께 누른 단추가 없어져
    // 초점이 문서로 빠지므로 "내 입력으로 다시 저장"으로 돌린다. 폼의 "저장"은 그대로 있으므로 두고(2차 검증 nit),
    // "내 입력으로 다시 저장"·덮어쓰기에서 바로 보낸 저장은 `saveMerged`가 돌린다.
    const refocus = pending !== undefined;
    const result = await api.updateTask(
      editingUser,
      task.id,
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
      const plan = await api.previewSchedule(task.id, input.start_date, input.due_date);
      if (plan.shifts.length === 0) {
        pending = undefined;
        await save(input, baseVersion, [], plan.undated, true);
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
    if (task === undefined) return;
    const fresh = await api.task(task.id);
    if (task.version === fresh.version) {
      task = fresh;
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
    if (task === undefined || hiddenForAccount) return;
    const identity = recurrenceIdentity;
    const id = task.id;
    const stillCurrent = (): boolean =>
      mounted && identity === recurrenceIdentity && task?.id === id;
    try {
      const base = latestRule ?? task;
      const fresh = await api.setTaskRecurrence(
        editingUser,
        id,
        rule,
        base.series?.timezone ?? localTimeZone(),
        baseRule === undefined ? (base.series?.rule ?? null) : baseRule,
      );
      if (!stillCurrent()) return;
      latestRule = undefined;
      if (task.version === fresh.version) task = fresh;
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
    if (task === undefined || busy) return;
    busy = true;
    error = undefined;
    try {
      const fresh = await api.removePredecessor(editingUser, task.id, predecessor);
      if (task.version === fresh.version) task = fresh;
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
    const link = task?.predecessors.find((l) => l.task_id === predecessor);
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
    if (task === undefined) return;
    const result = await api.setPredecessor(
      editingUser,
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
      // 아무것도 바뀌지 않았다. 지금 상태로 다시 계산해 다시 묻는다. 이제 옮길 것이 없으면 묻지 않고 그대로 저장하고
      // "밀릴 업무가 없어 그대로 저장"을 알린다(D-91 개정·D-125, R5 최종 A FA-2).
      const plan = await api.previewLink(task.id, predecessor, kind, lag);
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
    if (task.version === fresh.version) task = fresh;
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
      const fresh = await api.setTaskParent(editingUser, id, parentId);
      if (parentChoice === choiceAtStart) parentChoice = '';
      if (task?.version === fresh.version) task = fresh;
      else refresh(fresh);
      notifyDone(m.tasks_parent_changed());
      onchanged();
    } catch (e: unknown) {
      if (e instanceof ApiError && e.code === 'account_changed') accountBlocked = true;
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
{#if hiddenForAccount}
  <p role="alert">{m.tasks_account_changed()}</p>
  <Button onclick={() => void verifyAccount()}>{m.tasks_account_verify()}</Button>
{/if}
<div class="@container" hidden={hiddenForAccount} inert={hiddenForAccount}>
  {#if task === undefined && error === undefined}
    <!-- 읽는 동안 빈 창으로 두지 않는다. -->
    <p class="py-6 text-center text-sm text-muted-foreground" role="status">
      {m.modules_loading()}
    </p>
  {/if}
  {#if task !== undefined && form !== undefined}
    {@const current = task}
    {@const f = form}
    <!-- 덮어쓰기 확인이 떠 있는 동안에는 충돌 알림(확인 포함) 말고는 모두 잠근다. 체크리스트·반복·선행·상위 업무를
       바꾸면 다시 채우기가 확인을 말없이 닫고, 보인 값과 저장할 값이 어긋날 수 있다. -->
    <div class="flex flex-col gap-4">
      <div class="flex flex-wrap items-baseline gap-2" inert={clashAsk !== undefined}>
        <span class="font-mono text-sm text-muted-foreground">{current.reference ?? ''}</span>
        {#if current.access === 'read'}<span class="text-sm text-muted-foreground"
            >{m.tasks_read_only()}</span
          >{/if}
        <div class="ml-auto flex flex-wrap items-center gap-1">
          <MarkButtons kind="task" id={current.id} />
          {#if canEdit}
            <DeleteButton
              what={m.tasks_noun_task()}
              remove={() => api.trashTask(editingUser, current.id)}
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
        <form
          class="grid grid-cols-1 gap-3 *:min-w-0 @md:grid-cols-2"
          inert={pending !== undefined}
          onsubmit={(e) => {
            e.preventDefault();
            void submit(current);
          }}
        >
          <div class="@md:col-span-2 flex flex-col gap-1">
            <Label for="task-title">{m.tasks_title()}</Label>
            <Input id="task-title" bind:value={f.title} disabled={locked} maxlength={200} />
          </div>
          <div class="flex min-w-0 flex-col gap-1">
            <span class="text-sm font-medium" id="task-status-label">{m.tasks_status()}</span>
            <Select.Root
              type="single"
              value={f.status}
              disabled={locked}
              onValueChange={(v: string) => {
                const s = statusNames.find((x) => x === v);
                if (s !== undefined) f.status = s satisfies TaskStatus;
              }}
            >
              <Select.Trigger aria-labelledby="task-status-label">{f.status}</Select.Trigger>
              <Select.Content>
                {#each statusNames as s (s)}
                  <Select.Item value={s} label={s}>{s}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
          </div>
          <div class="flex min-w-0 flex-col gap-1">
            <span class="text-sm font-medium" id="task-priority-label">{m.tasks_priority()}</span>
            <Select.Root
              type="single"
              value={f.priority}
              disabled={locked}
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
              disabled={locked || f.milestone}
            />
          </div>
          <div class="flex min-w-0 flex-col gap-1">
            <Label for="task-due">{m.tasks_due_date()}</Label>
            <Input id="task-due" type="date" bind:value={f.due_date} disabled={locked} />
          </div>
          <div class="@md:col-span-2 flex flex-col gap-1">
            <Label for="task-description">{m.tasks_description()}</Label>
            <Textarea
              id="task-description"
              bind:value={f.description}
              disabled={locked}
              rows={3}
              class="min-h-20"
            />
          </div>
          <div class="flex min-w-0 flex-col gap-1">
            <Label for="task-estimate">{m.tasks_estimate()}</Label>
            <Input
              id="task-estimate"
              inputmode="numeric"
              bind:value={estimateText}
              disabled={locked}
            />
          </div>
          <div class="flex min-w-0 flex-col gap-1">
            <Label for="task-tags">{m.tasks_tags()}</Label>
            <Input id="task-tags" bind:value={tagsText} disabled={locked} />
          </div>
          <div class="@md:col-span-2 flex items-center gap-2">
            <Checkbox
              id="task-milestone"
              checked={f.milestone}
              disabled={locked}
              onCheckedChange={(v) => {
                f.milestone = v === true;
                if (f.milestone) f.start_date = null;
              }}
            />
            <Label for="task-milestone">{m.tasks_milestone()}</Label>
          </div>
          <fieldset class="@md:col-span-2 flex flex-col gap-2">
            <legend class="text-sm font-medium">{m.tasks_assignees()}</legend>
            <div class="flex flex-wrap gap-3">
              {#if peopleError !== undefined}
                <span class="text-sm text-destructive" role="alert">{peopleError}</span>
              {:else if people === undefined}
                <span class="text-sm text-muted-foreground" role="status"
                  >{m.modules_loading()}</span
                >
              {:else}
                {#each people as person (person.user_id)}
                  <span class="flex items-center gap-1.5">
                    <Checkbox
                      id={`assignee-${person.user_id}`}
                      checked={f.assignee_ids.includes(person.user_id)}
                      disabled={locked}
                      onCheckedChange={(v) => {
                        toggleAssignee(person.user_id, v === true);
                      }}
                    />
                    <Label for={`assignee-${person.user_id}`}
                      >{peopleLabels.get(person.user_id) ?? person.display_name}</Label
                    >
                  </span>
                {:else}
                  <span class="text-sm text-muted-foreground">{m.tasks_no_assignable()}</span>
                {/each}
              {/if}
            </div>
          </fieldset>
          {#if canEdit}
            <div class="@md:col-span-2 flex items-center gap-2">
              <Button id="task-save" type="submit" disabled={busy}>{m.common_save()}</Button>
            </div>
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
                  void changeParent(current.id, chosen === TOP ? null : chosen);
                }}>{m.tasks_parent_change()}</Button
              >
            </div>
          {/if}
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
                <span class="shrink-0 font-mono text-xs whitespace-nowrap text-muted-foreground"
                  >{sub.reference ?? ''}</span
                >
                <span class="min-w-0 [overflow-wrap:anywhere]">{sub.title}</span>
                <span class="ml-auto shrink-0 text-xs whitespace-nowrap text-muted-foreground"
                  >{sub.status}</span
                >
              </button>
            {:else}
              <p class="text-sm text-muted-foreground">{m.tasks_none()}</p>
            {/each}
            {#if canEdit}
              {#key childSeed}
                <CreateForm
                  id="new-subtask"
                  label={m.tasks_subtask_title()}
                  placeholder={m.tasks_subtask_placeholder()}
                  submitLabel={m.tasks_subtask_add()}
                  initial={childInitial}
                  onchange={(draft: { name: string; requestId: string | null } | undefined) => {
                    childDraft = draft;
                  }}
                  create={async (requestId: string, title: string) => {
                    await api.createTask(
                      editingUser,
                      current.item_id,
                      requestId,
                      title,
                      current.id,
                    );
                    onchanged();
                  }}
                />
              {/key}
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
            <!-- 저장 중에는 칸 전체를 잠근다. 성공 뒤 resetLinkEditor()가 그사이 친 값을 지우지 않게 한다(FA-1, D-97).
                 bits-ui Select는 조상 fieldset의 disabled를 보지 않아 자기 disabled로 잠근다. -->
            <fieldset
              class="flex flex-col gap-2 rounded-md border p-2"
              inert={linkPending !== undefined}
              disabled={busy}
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
                  disabled={busy}
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
                    disabled={busy}
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
              <p id="link-lag-hint" class="text-xs text-muted-foreground">
                {m.tasks_link_lag_hint()}
              </p>
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
        </section>

        <section class="flex flex-col gap-2" aria-labelledby="successors-heading">
          <h3 id="successors-heading" class="text-sm font-semibold">
            {m.tasks_successors_heading()}
          </h3>
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
            {#key current.id}
              <RecurrenceEditor
                bind:this={recurrenceEditor}
                idPrefix="task-recurrence"
                draftName={`task-recurrence:${current.id}`}
                rule={(latestRule ?? current).series?.rule ?? null}
                canEdit={canEdit && (current.due_date !== null || current.start_date !== null)}
                zoneNote={m.tasks_recurrence_zone({
                  zone: current.series?.timezone ?? localTimeZone(),
                })}
                onsave={saveRule}
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
            {/key}
            {#if canEdit && current.due_date === null && current.start_date === null}
              <p class="text-sm text-muted-foreground">{m.tasks_recurrence_needs_date()}</p>
            {/if}
          {/if}
        {/if}

        <ChecklistForm
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
              api.addChecklist(
                user,
                current.id,
                entries.map((e) => ({ request_id: e.requestId, text: e.text, checked: e.done })),
              ),
            )}
          ontoggle={(id: string, done: boolean, user: string) =>
            reloadChecklist(api.updateChecklistItem(user, id, { checked: done }))}
          onrename={(id: string, text: string, user: string) =>
            reloadChecklist(api.updateChecklistItem(user, id, { text }))}
          onremove={(id: string, user: string) =>
            reloadChecklist(api.deleteChecklistItem(user, id))}
        />

        <TaskTimePanel taskId={current.id} />
        <!-- 댓글 입력은 서버 접근 등급으로 연다(D-113). -->
        <CommentsPanel
          bind:this={commentsEditor}
          target={{ kind: 'task', id: current.id }}
          canWrite={current.access !== 'read'}
          itemId={current.item_id}
          onunsaved={(value: boolean) => {
            commentsUnsaved = value;
          }}
          ondraftkept={(value: boolean) => {
            commentsDraftKept = value;
          }}
        />

        <Backlinks kind="task" id={current.id} {workspaces} />
      </div>
    </div>
  {/if}
</div>
