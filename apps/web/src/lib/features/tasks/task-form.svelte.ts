import type { TaskDetailView, TaskInput } from './model';
import { SvelteSet } from 'svelte/reactivity';
import { canonicalJson } from '$lib/features/documents/drafts';
import {
  edited,
  estimateOf,
  formOf,
  mergeEstimate,
  mergeForm,
  scheduleOf,
  trimLikeServer,
  type MergeUnit,
} from './form';

/** 지금 입력을 기준 판에서 서버의 새 판으로 옮긴 결과(`mergeOnto`). */
export interface Merge {
  freshForm: TaskInput;
  form: TaskInput;
  baseline: TaskInput;
  /** 나와 서버가 서로 다른 값으로 바꾼 단위(일정은 시작일·마감일·마일스톤 한 묶음). */
  clashes: MergeUnit[];
  keepTagsText: boolean;
  keepEstimateText: boolean;
}

/** The body input, its server baseline and merge rules have a single owner. */
export function createTaskForm() {
  let task = $state<TaskDetailView | undefined>(undefined);
  let form = $state<TaskInput | undefined>(undefined);
  let tagsText = $state('');
  let estimateText = $state('');
  let baseline = $state.raw<TaskInput | undefined>(undefined);
  let baseVersion = 0;
  let clashed = false;

  function fill(next: TaskDetailView): void {
    task = next;
    form = formOf(next);
    baseline = formOf(next);
    baseVersion = next.version;
    clashed = false;
    tagsText = next.tags.join(', ');
    estimateText = next.estimate_minutes === null ? '' : String(next.estimate_minutes);
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
    if (!merge.keepTagsText) tagsText = merge.freshForm.tags.join(', ');
    if (!merge.keepEstimateText) {
      const fresh = merge.freshForm.estimate_minutes;
      estimateText = fresh === null ? '' : String(fresh);
    }
  }

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
        ...new SvelteSet(
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

  function toggleAssignee(id: string, on: boolean): void {
    if (form === undefined) return;
    form.assignee_ids = on
      ? [...new SvelteSet([...form.assignee_ids, id])]
      : form.assignee_ids.filter((x) => x !== id);
  }
  return {
    get task() {
      return task;
    },
    set task(value: typeof task) {
      task = value;
    },
    get form() {
      return form;
    },
    set form(value: typeof form) {
      form = value;
    },
    get tagsText() {
      return tagsText;
    },
    set tagsText(value: typeof tagsText) {
      tagsText = value;
    },
    get estimateText() {
      return estimateText;
    },
    set estimateText(value: typeof estimateText) {
      estimateText = value;
    },
    get baseline() {
      return baseline;
    },
    set baseline(value: typeof baseline) {
      baseline = value;
    },
    get baseVersion() {
      return baseVersion;
    },
    set baseVersion(value: typeof baseVersion) {
      baseVersion = value;
    },
    get clashed() {
      return clashed;
    },
    set clashed(value: typeof clashed) {
      clashed = value;
    },
    get changed() {
      return changed;
    },
    fill,
    mergeOnto,
    applyMerge,
    inputOf,
    currentInput,
    toggleAssignee,
  };
}
