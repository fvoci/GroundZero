// 업무 상태·우선순위의 표시 이름. 값은 읽을 때 지금 언어로 만든다(언어는 실행 중에 바뀔 수 있다, D-81).
import type { TaskPriority, TaskStatus } from '$lib/api/client';
import { m } from '$lib/paraglide/messages.js';

export const STATUS_LABEL: Record<TaskStatus, string> = {
  get todo() {
    return m.tasks_status_todo();
  },
  get in_progress() {
    return m.tasks_status_in_progress();
  },
  get done() {
    return m.tasks_status_done();
  },
};

export const PRIORITY_LABEL: Record<TaskPriority, string> = {
  get none() {
    return m.tasks_priority_none();
  },
  get low() {
    return m.tasks_priority_low();
  },
  get medium() {
    return m.tasks_priority_medium();
  },
  get high() {
    return m.tasks_priority_high();
  },
  get urgent() {
    return m.tasks_priority_urgent();
  },
};
