// 업무 상태·우선순위의 표시 이름.
import type { TaskPriority, TaskStatus } from '$lib/api/client';

export const STATUS_LABEL: Record<TaskStatus, string> = {
  todo: '할 일',
  in_progress: '진행 중',
  done: '완료',
};

export const PRIORITY_LABEL: Record<TaskPriority, string> = {
  none: '없음',
  low: '낮음',
  medium: '보통',
  high: '높음',
  urgent: '긴급',
};
