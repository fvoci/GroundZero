// 업무 CSV 일괄 등록(D-39)의 줄별 진행 상태. 한 줄은 모든 칸을 실은 만들기 요청 하나다(D-105): 서버가 한 트랜잭션에서
// 확인·저장하므로 거절된 줄은 아무것도 남기지 않는다. 다시 누를 때 줄마다 알아야 하는 것은 다 넣었는지와 요청 id뿐이다:
// 다 넣은 줄은 건너뛰고(같은 요청 id는 지금 업무를 돌려줄 뿐이지만 보낼 까닭이 없다), 아직인 줄은 같은 요청 id로 다시
// 보낸다(응답을 잃은 만들기가 서버에 반영됐으면 그 업무를 돌려받고 새로 만들지 않는다).
import type { TaskView } from '$lib/api/client';
import { newRequestId } from '$lib/api/client';
import { canonicalJson } from '$lib/features/documents/drafts';

import type { TaskRow } from './csv';

export interface RowProgress {
  /**
   * 만들기 요청 id. 제목이 같은 동안 이 id를 지킨다(만든 업무가 지워지면 `resetRow`). 실패한 줄을 고쳐도 같은 id를 쓴다:
   * 거절된 만들기는 아무것도 남기지 않아 고친 내용으로 새로 만들고, 응답을 잃은 만들기가 반영됐으면 그 업무가 돌아온다
   * (`rowApplied`로 고친 내용과 견준다).
   */
  requestId: string;
  readonly title: string;
  /** 넣은 줄 내용(`canonicalJson`). 아직이면 undefined. */
  applied: string | undefined;
}

export interface RowPair {
  row: TaskRow;
  /** 줄 내용의 비교 키(`canonicalJson`). */
  key: string;
  /** 이어받을 지난 상태. 없으면 새 줄. */
  progress: RowProgress | undefined;
}

export interface RowStep extends RowPair {
  progress: RowProgress;
}

/**
 * 이번 줄들에 지난 진행 상태를 짝짓는다. 내용이 그대로인 줄은 다 넣은 그 줄의 상태(건너뛴다), 그 밖의 줄은 같은 제목의
 * 아직 다 넣지 못한 상태(실패한 줄을 고친 경우: 같은 요청 id로 보낸다). 다 넣은 줄은 내용이 같은 줄에만 준다.
 * 같은 상태를 두 줄에 주지 않는다.
 */
export function pairRows(rows: readonly TaskRow[], known: readonly RowProgress[]): RowPair[] {
  const free = [...known];
  const take = (found: (p: RowProgress) => boolean): RowProgress | undefined => {
    const at = free.findIndex(found);
    return at < 0 ? undefined : free.splice(at, 1)[0];
  };
  const exact = rows.map((row) => {
    const key = canonicalJson(row);
    return { row, key, progress: take((p) => p.applied === key) };
  });
  return exact.map(({ row, key, progress }) => ({
    row,
    key,
    progress: progress ?? take((p) => p.applied === undefined && p.title === row.title),
  }));
}

/** `pairRows`에 짝이 없는 줄은 새 요청 id로 시작한다. */
export function matchRows(rows: readonly TaskRow[], known: readonly RowProgress[]): RowStep[] {
  return pairRows(rows, known).map(({ row, key, progress }) => ({
    row,
    key,
    progress: progress ?? { requestId: newRequestId(), title: row.title, applied: undefined },
  }));
}

/**
 * 이 줄의 요청 id로 만든 업무가 지워졌을 때(만들기 409 `task_deleted`) 줄을 처음 상태로 되돌린다. 같은 요청 id로 만들면
 * 서버가 지운 업무를 다시 찾아 같은 오류를 내므로 새 요청 id를 쓴다. 404(항목을 볼 수 없음)에는 부르지 않는다: 응답을 잃은
 * 만들기가 반영됐을 수 있어 새 id로 보내면 다시 볼 수 있게 된 뒤 같은 줄이 둘이 된다.
 */
export function resetRow(progress: RowProgress): void {
  progress.applied = undefined;
  progress.requestId = newRequestId();
}

/**
 * 서버의 업무가 이 줄 내용인지. 같은 요청 id의 만들기는 처음 만든 업무를 지금 상태로 돌려주므로(내용이 달라도), 응답을
 * 잃은 줄을 고쳐 다시 누르면 고치기 전 내용의 업무가 돌아온다. 서버가 저장하며 바꾸는 대로 맞춰 본다: 같은 태그는 앞의
 * 것만 남고(validate_tags), 담당자는 이름순으로 오므로 순서 없이 본다. CSV는 이정표를 늘 끄므로 켜져 있으면 다른 곳에서
 * 바꾼 것이다. 다르게 읽히면 이 줄과 다르다고 알린다(안전한 쪽).
 */
export function rowApplied(row: TaskRow, task: TaskView): boolean {
  const people = new Set(task.assignees.map((a) => a.user_id));
  const wanted = new Set(row.assignee_ids);
  return (
    task.title === row.title &&
    task.description === row.description &&
    task.status === row.status &&
    task.priority === row.priority &&
    task.start_date === row.start_date &&
    task.due_date === row.due_date &&
    task.estimate_minutes === row.estimate_minutes &&
    !task.milestone &&
    canonicalJson(task.tags) === canonicalJson([...new Set(row.tags)]) &&
    people.size === wanted.size &&
    [...wanted].every((id) => people.has(id))
  );
}

/** 다시 누르면 건너뛸(이미 다 넣은) 줄 수. */
export function resumeCount(rows: readonly TaskRow[], known: readonly RowProgress[]): number {
  return pairRows(rows, known).filter(({ key, progress }) => progress?.applied === key).length;
}
