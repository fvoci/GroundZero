import type { TaskInput } from './model';
import { MERGE_UNITS, same, scheduleOf } from './form';
import { fieldLabel, fieldText } from './labels';
import type { ConflictField, FieldSide } from '$lib/features/common/field-conflict';

export interface TaskComparison {
  base: TaskInput;
  mine: TaskInput;
  server: TaskInput;
  version: number;
  baseVersion: number;
  tagsText: string;
  estimateText: string;
  choices: Record<string, FieldSide>;
  original: TaskInput;
  originalBase: TaskInput;
  originalVersion: number;
  originalTagsText: string;
  originalEstimateText: string;
  repeated: boolean;
}
export function taskConflictFields(names: ReadonlyMap<string, string>): ConflictField<TaskInput>[] {
  return MERGE_UNITS.map((key) => ({
    key,
    label: fieldLabel(key),
    same: (a, b) => same(key, a, b),
    text: (input) =>
      key === 'tags' && input.tags.length > 0
        ? input.tags.map((tag) => JSON.stringify(tag)).join(', ')
        : fieldText(key, input, names),
    copy: (target, source) => {
      if (key === 'schedule') Object.assign(target, scheduleOf(source));
      else Object.assign(target, { [key]: source[key] });
    },
  }));
}
