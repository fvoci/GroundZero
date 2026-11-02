import { TASK_PRIORITIES, type TaskInput } from './model';
import type { TaskComparison } from './field-conflict';
export interface TaskDraft {
  baseVersion: number;
  baseline: TaskInput;
  form: TaskInput;
  tagsText: string;
  estimateText: string;
  child: { name: string; requestId: string | null } | null;
  savedAt: string;
  comparison?: TaskComparison | undefined;
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
    comparison: 'comparison' in value ? parseComparison(value.comparison) : undefined,
  };
}

function parseComparison(value: unknown): TaskComparison | undefined {
  if (
    typeof value !== 'object' ||
    value === null ||
    !('base' in value && isTaskInput(value.base)) ||
    !('mine' in value && isTaskInput(value.mine)) ||
    !('server' in value && isTaskInput(value.server)) ||
    !('original' in value && isTaskInput(value.original)) ||
    !('version' in value && typeof value.version === 'number') ||
    !('baseVersion' in value && typeof value.baseVersion === 'number') ||
    !('tagsText' in value && typeof value.tagsText === 'string') ||
    !('estimateText' in value && typeof value.estimateText === 'string') ||
    !('originalBase' in value && isTaskInput(value.originalBase)) ||
    !('originalVersion' in value && typeof value.originalVersion === 'number') ||
    !('originalTagsText' in value && typeof value.originalTagsText === 'string') ||
    !('originalEstimateText' in value && typeof value.originalEstimateText === 'string') ||
    !('choices' in value && typeof value.choices === 'object' && value.choices !== null)
  )
    return undefined;
  const choices = Object.fromEntries(
    Object.entries(value.choices).filter(
      (entry): entry is [string, 'mine' | 'server'] => entry[1] === 'mine' || entry[1] === 'server',
    ),
  );
  return {
    base: value.base,
    mine: value.mine,
    server: value.server,
    original: value.original,
    originalBase: value.originalBase,
    originalVersion: value.originalVersion,
    originalTagsText: value.originalTagsText,
    originalEstimateText: value.originalEstimateText,
    version: value.version,
    baseVersion: value.baseVersion,
    tagsText: value.tagsText,
    estimateText: value.estimateText,
    choices,
    repeated: true,
  };
}
