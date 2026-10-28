import type { StatusCategory } from '@fvoci/api-client';
import { EXPECTED_USER, request, expectOk, fail, ApiError } from '$lib/shared/api';
import { int, isRecord, none, parseList, parseTaskStatusDefinition, rec, str, type Json } from '$lib/shared/api/parse';
import { switchAccount } from '$lib/features/feedback/feedback.svelte';
import { m } from '$lib/paraglide/messages.js';
import { parseTask, parseTaskDetail, parseAssignee, parseSchedulePlan, TaskRuleConflict, type TaskInput, type TaskDetailView, type TaskStatus, type SuccessorShift, type RecurrenceRule, type LinkKind, type LinkBase } from './model';
// 업무 요청의 409 account_changed: 다른 요청과 똑같이 앱 전체 계정 전환(안전 상태·다시 읽기)을 시작한다(D-96; 앞 계정의
// 화면을 그대로 두면 다른 화면이 새 계정의 세션으로 요청을 보낸다). 업무 폼은 이 오류로 입력을 숨기고, 입력은 앞 계정의
// 브라우저 초안(common/form-draft.ts, 사용자별 키)으로 남아 그 계정이 다시 열면 되살릴 수 있다. 모든 업무 쓰기는 편집을 시작한
// 사용자를 명시적으로 보내며, 다른 409(conflict 등)는 그대로 폼이 받는다.
function failTask(status: number, json: Json): never {
  if (status === 409 && isRecord(json) && json.error === 'account_changed') {
    switchAccount();
    throw new ApiError(409, 'account_changed', m.common_error_account_changed());
  }
  fail(status, json);
}

async function expectTask<T>(
  method: string,
  path: string,
  parse: (v: Json) => T,
  body: Json,
  expectedUser: string,
  // 선택적 취소 신호(#84 CSV 가져오기 중단). 기존 호출은 그대로.
  signal?: AbortSignal,
): Promise<T> {
  const { status, json } = await request(
    method,
    path,
    body,
    { [EXPECTED_USER]: expectedUser },
    signal,
  );
  if (status < 200 || status >= 300) failTask(status, json);
  return parse(json);
}
export const workApi = {
  /** 항목의 상태 정의(D-116). 칸반 열·상태 편집의 정본. */
  taskStatuses: (itemId: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(itemId)}/statuses`, (v) =>
      parseList(v, parseTaskStatusDefinition),
    ),
  createTaskStatus: (
    expectedUser: string,
    itemId: string,
    name: string,
    color: string,
    category: StatusCategory,
  ) =>
    expectTask(
      'POST',
      `/api/items/${encodeURIComponent(itemId)}/statuses`,
      parseTaskStatusDefinition,
      { name, color, category },
      expectedUser,
    ),
  /** 바꾸는 칸만 보낸다. `category`를 done으로 바꾸면 그 열 업무의 완료 시각이 채워진다. */
  updateTaskStatus: (
    expectedUser: string,
    itemId: string,
    statusId: string,
    patch: { name?: string; color?: string; category?: StatusCategory; position?: number },
  ) =>
    expectTask(
      'PUT',
      `/api/items/${encodeURIComponent(itemId)}/statuses/${encodeURIComponent(statusId)}`,
      parseTaskStatusDefinition,
      patch,
      expectedUser,
    ),
  /** 업무가 있는 열은 `moveTo`(같은 항목의 다른 상태)가 필요하다. 옮긴 업무 수를 돌려준다. */
  deleteTaskStatus: (
    expectedUser: string,
    itemId: string,
    statusId: string,
    moveTo: string | null,
  ) =>
    expectTask(
      'DELETE',
      `/api/items/${encodeURIComponent(itemId)}/statuses/${encodeURIComponent(statusId)}${
        moveTo === null ? '' : `?move_to=${encodeURIComponent(moveTo)}`
      }`,
      (v) => int(rec(v, 'deleted status'), 'moved'),
      undefined,
      expectedUser,
    ),
  tasks: (itemId: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(itemId)}/tasks`, (v) =>
      parseList(v, parseTask),
    ),
  assignable: (itemId: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(itemId)}/assignable`, (v) =>
      parseList(v, parseAssignee),
    ),
  task: (id: string) => expectOk('GET', `/api/tasks/${encodeURIComponent(id)}`, parseTaskDetail),
  /**
   * 업무 만들기. `fields`를 주면 그 칸까지 서버가 한 트랜잭션에서 확인·저장한다(D-105, 하나라도 거절되면 업무가 남지
   * 않는다). 같은 요청 id는 처음 만든 업무를 지금 상태로 돌려준다(보낸 칸이 달라도). `expectedUser`는 편집 시작 시 고정한다.
   */
  createTask: (
    expectedUser: string,
    itemId: string,
    requestId: string,
    title: string,
    parentId: string | null = null,
    fields?: Partial<Omit<TaskInput, 'title'>>,
    // #84 가져오기 중단.
    signal?: AbortSignal,
  ) =>
    expectTask(
      'POST',
      `/api/items/${encodeURIComponent(itemId)}/tasks`,
      parseTaskDetail,
      {
        ...fields,
        request_id: requestId,
        title,
        parent_id: parentId,
      },
      expectedUser,
      signal,
    ),
  // 빠른 추가는 CreateTask의 기본값을 쓰고 담당=나만 명시한다(D-120).
  createQuickTask: (
    expectedUser: string,
    itemId: string,
    requestId: string,
    title: string,
    userId: string,
  ) =>
    expectTask(
      'POST',
      `/api/items/${encodeURIComponent(itemId)}/tasks`,
      parseTaskDetail,
      {
        request_id: requestId,
        title,
        assignee_ids: [userId],
      },
      expectedUser,
    ),
  /**
   * 필드 고치기와 승인한 후속 재배치. 이 업무의 기준 버전이 지났으면 `conflict`, 승인한 후속 업무가 그사이
   * 바뀌었으면 `shift_conflict`(아무것도 바뀌지 않았으니 다시 미리 본다).
   */
  async updateTask(
    expectedUser: string,
    id: string,
    baseVersion: number,
    input: TaskInput,
    successorShifts: SuccessorShift[] = [],
  ): Promise<
    | { kind: 'updated'; task: TaskDetailView }
    | { kind: 'conflict'; current: TaskDetailView }
    | { kind: 'shift_conflict'; current: TaskDetailView }
  > {
    const { status, json } = await request(
      'PUT',
      `/api/tasks/${encodeURIComponent(id)}`,
      {
        base_version: baseVersion,
        ...input,
        successor_shifts: successorShifts,
      },
      { [EXPECTED_USER]: expectedUser },
    );
    // 409 중 기준 버전 충돌만 서버 최신본이 온다(보관·다시 시도 등은 일반 오류).
    if (status === 409 && isRecord(json) && json.current !== undefined) {
      const body = rec(json, 'conflict');
      const current = parseTaskDetail(body.current);
      return str(body, 'error') === 'shift_conflict'
        ? { kind: 'shift_conflict', current }
        : { kind: 'conflict', current };
    }
    if (status !== 200) failTask(status, json);
    return { kind: 'updated', task: parseTaskDetail(json) };
  },
  /**
   * 칸반 이동. `baseMoveVersion`을 주면 그 이동 판에서 읽은 업무만 옮긴다(D-89): 그사이 열·위치가 바뀌었으면 서버가
   * 아무것도 바꾸지 않고 409 `move_conflict`를 돌려준다(`ApiError`). 주지 않으면 판과 상관없이 옮긴다.
   */
  moveTask: (
    expectedUser: string,
    id: string,
    status: TaskStatus,
    beforeId: string | null,
    baseMoveVersion?: number,
  ) =>
    expectTask(
      'PUT',
      `/api/tasks/${encodeURIComponent(id)}/position`,
      parseTaskDetail,
      {
        status,
        before_id: beforeId,
        ...(baseMoveVersion === undefined ? {} : { base_move_version: baseMoveVersion }),
      },
      expectedUser,
    ),
  /** 날짜를 바꿀 때의 재배치 계획(옮길 후속 업무와 원인 관계, 날짜가 없어 계산하지 않은 업무). */
  previewSchedule: (id: string, startDate: string | null, dueDate: string | null) =>
    expectOk('POST', `/api/tasks/${encodeURIComponent(id)}/schedule-plan`, parseSchedulePlan, {
      start_date: startDate,
      due_date: dueDate,
    }),
  /** 관계를 만들거나 종류·지연일을 바꿀 때의 재배치 계획(아무것도 바꾸지 않는다). */
  previewLink: (id: string, predecessorId: string, kind: LinkKind, lagDays: number) =>
    expectOk(
      'POST',
      `/api/tasks/${encodeURIComponent(id)}/predecessors/${encodeURIComponent(predecessorId)}/schedule-plan`,
      parseSchedulePlan,
      { kind, lag_days: lagDays },
    ),
  /**
   * 관계를 그 종류·지연일로 만들거나 바꾸고, 승인한 재배치를 함께 옮긴다(한 트랜잭션). `base`는 편집을 시작한 관계의
   * 종류·지연일이다(새 관계면 `null`). 그사이 관계가 바뀌었으면 `link_conflict`, 승인 목록이 맞지 않게 됐으면
   * `shift_conflict`(둘 다 아무것도 바뀌지 않았고 최신본을 준다).
   */
  async setPredecessor(
    expectedUser: string,
    id: string,
    predecessorId: string,
    kind: LinkKind,
    lagDays: number,
    base: LinkBase | null,
    successorShifts: SuccessorShift[] = [],
  ): Promise<
    | { kind: 'updated'; task: TaskDetailView }
    | { kind: 'shift_conflict'; current: TaskDetailView }
    | { kind: 'link_conflict'; current: TaskDetailView }
  > {
    const { status, json } = await request(
      'PUT',
      `/api/tasks/${encodeURIComponent(id)}/predecessors/${encodeURIComponent(predecessorId)}`,
      {
        kind,
        lag_days: lagDays,
        base: base === null ? null : { kind: base.kind, lag_days: base.lag_days },
        successor_shifts: successorShifts,
      },
      { [EXPECTED_USER]: expectedUser },
    );
    if (status === 409 && isRecord(json) && json.current !== undefined) {
      const body = rec(json, 'conflict');
      const error = str(body, 'error');
      if (error === 'shift_conflict' || error === 'link_conflict') {
        return { kind: error, current: parseTaskDetail(body.current) };
      }
    }
    if (status !== 200) fail(status, json);
    return { kind: 'updated', task: parseTaskDetail(json) };
  },
  removePredecessor: (expectedUser: string, id: string, predecessorId: string) =>
    expectTask(
      'DELETE',
      `/api/tasks/${encodeURIComponent(id)}/predecessors/${encodeURIComponent(predecessorId)}`,
      parseTaskDetail,
      undefined,
      expectedUser,
    ),
  setTaskParent: (expectedUser: string, id: string, parentId: string | null) =>
    expectTask(
      'PUT',
      `/api/tasks/${encodeURIComponent(id)}/parent`,
      parseTaskDetail,
      {
        parent_id: parentId,
      },
      expectedUser,
    ),
  addChecklist: (
    expectedUser: string,
    taskId: string,
    items: { request_id: string; text: string; checked: boolean }[],
  ) =>
    expectTask(
      'POST',
      `/api/tasks/${encodeURIComponent(taskId)}/checklist`,
      parseTaskDetail,
      {
        items,
      },
      expectedUser,
    ),
  /** 체크리스트 항목 고치기. 바꾸려는 칸만 보낸다(빠진 칸은 서버가 그대로 둔다). */
  updateChecklistItem: (
    expectedUser: string,
    id: string,
    change: { text: string } | { checked: boolean },
  ) =>
    expectTask(
      'PUT',
      `/api/checklist-items/${encodeURIComponent(id)}`,
      none,
      { ...change },
      expectedUser,
    ),
  deleteChecklistItem: (expectedUser: string, id: string) =>
    expectTask(
      'DELETE',
      `/api/checklist-items/${encodeURIComponent(id)}`,
      none,
      undefined,
      expectedUser,
    ),
  async setTaskRecurrence(
    expectedUser: string,
    id: string,
    rule: RecurrenceRule | null,
    timezone: string,
    base: RecurrenceRule | null,
  ): Promise<TaskDetailView> {
    const { status, json } = await request(
      'PUT',
      `/api/tasks/${encodeURIComponent(id)}/recurrence`,
      {
        rule: rule === null ? null : { ...rule },
        timezone,
        base: base === null ? null : { ...base },
      },
      { [EXPECTED_USER]: expectedUser },
    );
    if (
      status === 409 &&
      isRecord(json) &&
      json.error === 'rule_conflict' &&
      json.current !== undefined
    )
      throw new TaskRuleConflict(parseTaskDetail(json.current));
    if (status !== 200) failTask(status, json);
    return parseTaskDetail(json);
  },
  /** 반복 업무 회차 만들기(쓰기 권한으로 항목을 열 때). 만든 수. */
  syncRecurrences: (expectedUser: string, itemId: string) =>
    expectTask(
      'POST',
      `/api/items/${encodeURIComponent(itemId)}/recurrences/sync`,
      (v) => int(rec(v, 'sync'), 'created'),
      undefined,
      expectedUser,
    ),
  trashTask: (expectedUser: string, id: string) =>
    expectTask(
      'DELETE',
      `/api/tasks/${encodeURIComponent(id)}`,
      (v) => str(rec(v, 'trashed'), 'batch_id'),
      undefined,
      expectedUser,
    ),
};
