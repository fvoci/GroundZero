// 업무 CSV 일괄 등록(D-39)의 줄별 진행 상태. 한 줄은 만들기(제목만)와 고치기(나머지 칸) 두 요청이라, 다시 누를 때
// 줄마다 어디까지 했는지 알아야 한다: 다 넣은 줄은 건너뛰고(다시 고치기를 보내면 그사이 다른 사람이 바꾼 값을 CSV 값으로
// 되돌린다), 제목만 만들어진 줄은 그 업무에 이어서 채운다(새로 만들면 같은 업무가 둘이 된다).
import { newRequestId } from '$lib/api/client';
import { canonicalJson } from '$lib/features/documents/drafts';

import type { TaskRow } from './csv';

export interface RowProgress {
  /** 만들기 요청 id. 만들기는 제목만 보내므로 제목이 같은 동안 이 id를 지킨다. */
  readonly requestId: string;
  readonly title: string;
  /** 만든 업무와 마지막으로 받은 판(만들기·고치기 응답). 아직 만들지 못했으면 undefined. */
  task: { id: string; version: number } | undefined;
  /** 고치기까지 끝낸 줄 내용(`canonicalJson`). 아직이면 undefined(제목만 있는 업무일 수 있다). */
  applied: string | undefined;
}

export interface RowStep {
  row: TaskRow;
  /** 줄 내용의 비교 키(`canonicalJson`). */
  key: string;
  progress: RowProgress;
}

/**
 * 이번 줄들에 지난 진행 상태를 붙인다. 내용이 그대로인 줄은 그 줄의 상태(다 넣었으면 건너뛴다), 고친 줄은 같은 제목의
 * 남은 상태(아직 못 채운 것 먼저: 실패한 줄을 고친 경우), 새 줄은 새 요청 id. 같은 상태를 두 줄에 주지 않는다.
 */
export function matchRows(rows: readonly TaskRow[], known: readonly RowProgress[]): RowStep[] {
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
    progress: progress ??
      take((p) => p.title === row.title && p.applied === undefined) ??
      take((p) => p.title === row.title) ?? {
        requestId: newRequestId(),
        title: row.title,
        task: undefined,
        applied: undefined,
      },
  }));
}
