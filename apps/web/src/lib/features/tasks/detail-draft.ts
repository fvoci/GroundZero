import { TASK_PRIORITIES, type TaskInput } from './model';
export interface TaskDraft {
  baseVersion: number;
  baseline: TaskInput;
  form: TaskInput;
  tagsText: string;
  estimateText: string;
  child: { name: string; requestId: string | null } | null;
  savedAt: string;
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
export function parseTaskDraft(value: unknown): TaskDraft | undefined {
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
  if (
    typeof version !== 'number' ||
    typeof tags !== 'string' ||
    typeof estimate !== 'string' ||
    typeof savedAt !== 'string' ||
    !isTaskInput(before) ||
    !isTaskInput(kept)
  )
    return undefined;
  return {
    baseVersion: version,
    baseline: before,
    form: kept,
    tagsText: tags,
    estimateText: estimate,
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
