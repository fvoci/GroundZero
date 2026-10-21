// 업무 편집 폼 값. 체크리스트·반복·선행 업무를 바꾸면 서버의 새 판으로 다시 채우는데, 그때 사용자가 고쳤지만
// 아직 저장하지 않은 칸은 지킨다(전에는 폼 전체를 덮어 고친 제목·설명 등이 사라졌다).
import type { TaskDetailView, TaskInput } from '$lib/api/client';
import { canonicalJson } from '$lib/features/documents/drafts';

export function formOf(task: TaskDetailView): TaskInput {
  return {
    title: task.title,
    description: task.description,
    status: task.status,
    priority: task.priority,
    start_date: task.start_date,
    due_date: task.due_date,
    estimate_minutes: task.estimate_minutes,
    tags: task.tags,
    assignee_ids: task.assignees.map((a) => a.user_id),
    milestone: task.milestone,
  };
}

/**
 * 서버의 새 값(`fresh`)으로 다시 채우되, 기준 값(`before`, 마지막으로 불러온 판)에서 사용자가 바꾼 칸(`mine`)은
 * 그대로 둔다. 사용자가 바꾼 칸을 서버에서도 누가 다른 값으로 바꿨으면 `clash`이고 그 칸들이 `clashes`(폼 순서):
 * 다시 채울 때는 옛 판 번호로 저장해 충돌 화면(409)이 고르게 하고, 충돌 화면의 "내 입력으로 다시 저장"은 그 칸들을
 * 보이고 덮어쓸지 묻는다(D-94). `form`의 그 칸은 내 값이다. `baseline`은 다음 비교의 기준: 사용자가 바꾸지 않은 칸만
 * 새 값으로 옮긴다.
 */
export function mergeForm(
  before: TaskInput,
  mine: TaskInput,
  fresh: TaskInput,
): { form: TaskInput; baseline: TaskInput; clash: boolean; clashes: (keyof TaskInput)[] } {
  const clashes: (keyof TaskInput)[] = [];
  const same = (a: unknown, b: unknown): boolean => canonicalJson(a) === canonicalJson(b);
  const untouched = (key: keyof TaskInput): boolean => same(mine[key], before[key]);
  const pick = <K extends keyof TaskInput>(key: K): TaskInput[K] => {
    if (untouched(key) || same(mine[key], fresh[key])) return fresh[key];
    if (!same(fresh[key], before[key])) clashes.push(key);
    return mine[key];
  };
  // 사용자 값이 서버의 새 값과 같아진 칸도 기준을 새 값으로 옮긴다(그 뒤 사용자가 다시 고쳐도 옛 기준과 비교해 충돌로
  // 보지 않게).
  const keep = <K extends keyof TaskInput>(key: K): TaskInput[K] =>
    untouched(key) || same(mine[key], fresh[key]) ? fresh[key] : before[key];
  const baseline: TaskInput = {
    title: keep('title'),
    description: keep('description'),
    status: keep('status'),
    priority: keep('priority'),
    start_date: keep('start_date'),
    due_date: keep('due_date'),
    estimate_minutes: keep('estimate_minutes'),
    tags: keep('tags'),
    assignee_ids: keep('assignee_ids'),
    milestone: keep('milestone'),
  };
  const form: TaskInput = {
    title: pick('title'),
    description: pick('description'),
    status: pick('status'),
    priority: pick('priority'),
    start_date: pick('start_date'),
    due_date: pick('due_date'),
    estimate_minutes: pick('estimate_minutes'),
    tags: pick('tags'),
    assignee_ids: pick('assignee_ids'),
    milestone: pick('milestone'),
  };
  return { form, baseline, clash: clashes.length > 0, clashes };
}

/** 예상 시간 입력 글자의 값: 빈칸은 없음, 0 이상의 정수만 값, 그 밖은 잘못된 입력. */
export function estimateOf(text: string): number | null | 'invalid' {
  const t = text.trim();
  if (t === '') return null;
  return /^\d+$/.test(t) ? Number.parseInt(t, 10) : 'invalid';
}

/**
 * 다시 채울 때의 예상 시간 칸. 예상 시간은 글자로 입력하므로 `mergeForm`과 따로, 입력한 글자의 값으로 판정한다. 값이
 * 기준과 같으면("01"과 1) 고치지 않은 칸이라 서버 값을 쓰고(`keepText` 거짓), 잘못된 입력("abc")은 고친 칸으로 지킨다.
 * 화면 글자와 병합이 같은 판정을 써야 저장할 때 서버의 새 값을 조용히 덮지 않는다.
 */
export function mergeEstimate(
  before: number | null,
  text: string,
  fresh: number | null,
): { keepText: boolean; baseline: number | null; clash: boolean } {
  const typed = estimateOf(text);
  const keepText = typed !== before && typed !== fresh;
  return {
    keepText,
    baseline: keepText ? before : fresh,
    clash: keepText && fresh !== before,
  };
}
