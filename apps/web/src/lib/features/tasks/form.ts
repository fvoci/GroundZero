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
 * 서버의 새 값(`fresh`)으로 다시 채우되, 처음 값(`before`)에서 사용자가 바꾼 칸(`mine`)은 그대로 둔다.
 * 사용자가 바꾼 칸을 서버에서도 누가 바꿨으면 `clash`: 그때는 옛 판 번호로 저장해 충돌 화면(409)이 고르게 한다.
 */
export function mergeForm(
  before: TaskInput,
  mine: TaskInput,
  fresh: TaskInput,
): { form: TaskInput; clash: boolean } {
  let clash = false;
  const same = (a: unknown, b: unknown): boolean => canonicalJson(a) === canonicalJson(b);
  const pick = <K extends keyof TaskInput>(key: K): TaskInput[K] => {
    if (same(mine[key], before[key]) || same(mine[key], fresh[key])) return fresh[key];
    if (!same(fresh[key], before[key])) clash = true;
    return mine[key];
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
  return { form, clash };
}
