// 업무 CSV 일괄 등록(D-39)의 줄별 진행 상태. 한 줄은 만들기(제목만)와 고치기(나머지 칸) 두 요청이라, 다시 누를 때
// 줄마다 어디까지 했는지 알아야 한다: 다 넣은 줄은 건너뛰고(다시 고치기를 보내면 그사이 다른 사람이 바꾼 값을 CSV 값으로
// 되돌린다), 제목만 만들어진 줄은 그 업무에 이어서 채운다(새로 만들면 같은 업무가 둘이 된다). 다 넣은 업무는 다시 고치지
// 않는다: 내용을 고친 줄이나 다음 묶음의 같은 제목 줄이 그 업무를 덮어쓰게 된다.
import type { TaskView } from '$lib/api/client';
import { newRequestId } from '$lib/api/client';
import { canonicalJson } from '$lib/features/documents/drafts';

import type { TaskRow } from './csv';

export interface RowProgress {
  /** 만들기 요청 id. 만들기는 제목만 보내므로 제목이 같은 동안 이 id를 지킨다(만든 업무가 지워지면 `resetRow`). */
  requestId: string;
  readonly title: string;
  /** 만든 업무와 마지막으로 받은 판(만들기·고치기 응답). 아직 만들지 못했으면 undefined. */
  task: { id: string; version: number } | undefined;
  /** 고치기까지 끝낸 줄 내용(`canonicalJson`). 아직이면 undefined(제목만 있는 업무일 수 있다). */
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
 * 아직 다 넣지 못한 상태(실패한 줄을 고친 경우: 제목만 만들어진 그 업무에 채운다). 다 넣은 줄은 내용이 같은 줄에만 준다.
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
    progress: progress ?? {
      requestId: newRequestId(),
      title: row.title,
      task: undefined,
      applied: undefined,
    },
  }));
}

/**
 * 이 줄이 만든 업무가 지워졌거나 볼 수 없게 되었을 때(만들기·고치기 404) 줄을 처음 상태로 되돌린다. 그 업무에는 채울 수
 * 없고, 같은 요청 id로 만들면 서버가 지운 업무를 다시 찾아 404를 내므로(app_create_task는 휴지통을 거르지 않는다) 새
 * 요청 id를 쓴다. 항목이 없어서 난 404였다면 새 id도 404일 뿐이다.
 */
export function resetRow(progress: RowProgress): void {
  progress.task = undefined;
  progress.applied = undefined;
  progress.requestId = newRequestId();
}

/**
 * 서버의 업무가 이미 이 줄 내용인지. 고치기가 반영되고 응답만 잃은 줄은 다시 누를 때 옛 판으로 보내 겹침(409)을 받는데,
 * 그 업무가 이 줄과 같으면 다 넣은 것이다(겹침으로 알리면 덮어쓸 것이 없는데도 한 번 더 누르게 한다). 서버가 저장하며
 * 바꾸는 대로 맞춰 본다: 같은 태그는 앞의 것만 남고(validate_tags), 담당자는 이름순으로 오므로 순서 없이 본다. CSV는
 * 이정표를 늘 끄므로 켜져 있으면 다른 곳에서 바꾼 것이다. 다르게 읽히면 겹침으로 알린다(안전한 쪽).
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

/** 다시 누르면 이어서 넣을 줄 수: 이미 다 넣어 건너뛸 줄과 제목만 만들어진 업무에 채울 줄. */
export function resumeCounts(
  rows: readonly TaskRow[],
  known: readonly RowProgress[],
): { skip: number; fill: number } {
  let skip = 0;
  let fill = 0;
  for (const { key, progress } of pairRows(rows, known)) {
    if (progress === undefined) continue;
    if (progress.applied === key) skip += 1;
    else if (progress.task !== undefined) fill += 1;
  }
  return { skip, fill };
}
