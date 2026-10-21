import { describe, expect, it } from 'vitest';

import { ganttLayout, ganttLinks, MAX_GANTT_DAYS } from '$lib/features/tasks/gantt';

// 간트 연결선(끝나야 시작, D-31)의 좌표 계산. 막대와 같은 rem 좌표계: 열 i의 막대는 i*col+0.125에서 시작하고 끝 열 j는
// (j+1)*col-0.125에서 끝난다. 마일스톤 ◆는 열 가운데. 줄 i의 가운데는 (i+0.5)*row.
const G = { col: 2, row: 1.75, days: 12 };

interface Row {
  task: { id: string; milestone: boolean; predecessor_ids: string[] };
  from: number;
  to: number;
  clippedEnd?: boolean;
}
const row = (
  id: string,
  from: number,
  to: number,
  preds: string[] = [],
  milestone = false,
): Row => ({
  task: { id, milestone, predecessor_ids: preds },
  from,
  to,
});

/** 경로의 꼭짓점(M·H·V만 쓴다). */
function points(d: string): [number, number][] {
  const out: [number, number][] = [];
  let x = 0;
  let y = 0;
  for (const [, cmd, rest] of d.matchAll(/([MHV])([^MHV]+)/g)) {
    const nums = (rest ?? '').trim().split(/\s+/).map(Number);
    if (cmd === 'M') [x, y] = [nums[0] ?? 0, nums[1] ?? 0];
    else if (cmd === 'H') x = nums[0] ?? 0;
    else y = nums[0] ?? 0;
    out.push([x, y]);
  }
  return out;
}
/** 모든 구간이 수평·수직(직각 경로)인지, 마지막 구간이 왼쪽에서 오른쪽으로 들어가는지. */
function rightAngledInto(d: string, end: [number, number]): boolean {
  const p = points(d);
  const last = p.at(-1);
  const before = p.at(-2);
  if (last === undefined || before === undefined) return false;
  const straight = p.slice(1).every((q, i) => {
    const prev = p[i];
    return prev !== undefined && (q[0] === prev[0] || q[1] === prev[1]);
  });
  return (
    straight &&
    last[0] === end[0] &&
    last[1] === end[1] &&
    before[1] === last[1] &&
    before[0] < last[0]
  );
}

describe('gantt links', () => {
  it('joins the end of a predecessor bar to the start of the follower with an arrow', () => {
    const { links, hidden } = ganttLinks([row('a', 0, 2), row('b', 5, 6, ['a'])], G);
    expect(hidden).toBe(0);
    expect(links).toHaveLength(1);
    const [link] = links;
    expect(link?.key).toBe('a>b');
    // 끝 열 2(포함)의 오른쪽 끝 5.875 → 시작 열 5의 왼쪽 10.125. 날짜를 하루 더하거나 빼지 않는다.
    expect(link?.d).toBe('M5.875 0.875H6.275V2.625H10.125');
    expect(link?.arrow).toBe('M10.125 2.625L9.825 2.415L9.825 2.835Z');
  });

  it('draws a chain and many-to-many relations once each, up or down the rows', () => {
    const rows = [
      row('c', 8, 9, ['a', 'b', 'a']), // 'a'가 두 번: 한 번만
      row('a', 0, 1),
      row('b', 3, 4, ['a']),
      row('d', 10, 11, ['c', 'b']),
    ];
    const { links, hidden } = ganttLinks(rows, G);
    expect(hidden).toBe(0);
    expect(links.map((l) => l.key).sort()).toEqual(['a>b', 'a>c', 'b>c', 'b>d', 'c>d']);
    // 위쪽 줄(c, 0번)으로 가는 선도 직각으로 c의 시작에 왼쪽에서 들어간다.
    const up = links.find((l) => l.key === 'a>c');
    expect(up !== undefined && rightAngledInto(up.d, [16.125, 0.875])).toBe(true);
    const down = links.find((l) => l.key === 'b>d');
    expect(down !== undefined && rightAngledInto(down.d, [20.125, 5.25 + 0.875])).toBe(true);
  });

  it('steps in the middle of the gap when the follower starts the very next day', () => {
    // a는 열 2(포함)에서 끝나고 b는 열 3에서 시작: 두 막대 사이 틈은 여백 두 개(0.25rem)뿐이다.
    const next = ganttLinks([row('a', 0, 2), row('b', 3, 4, ['a'])], G).links[0];
    expect(next?.d).toBe('M5.875 0.875H6V2.625H6.125');
    expect(next !== undefined && rightAngledInto(next.d, [6.125, 2.625])).toBe(true);
  });

  it('routes along the row border when the follower starts the same day, overlaps or comes first', () => {
    // 같은 날: a는 열 2에서 끝나고 b는 열 2에서 시작한다(끝나야 시작을 어긴 일정이어도 그대로 그린다).
    const same = ganttLinks([row('a', 0, 2), row('b', 2, 3, ['a'])], G).links[0];
    expect(same?.d).toBe('M5.875 0.875H6.275V1.75H3.725V2.625H4.125');
    // 후속이 앞(역전)이고 위 줄: 선행 줄의 위 경계(1.75)를 따라간다.
    const back = ganttLinks([row('b', 0, 1, ['a']), row('a', 4, 6)], G).links[0];
    expect(back?.d).toBe('M13.875 2.625H14.275V1.75H0V0.875H0.125');
    // 첫 열에서 시작하는 후속: 경로와 화살촉이 표 왼쪽 끝(0)을 넘지 않는다.
    expect(back?.arrow).toBe('M0.125 0.875L0 0.665L0 1.085Z');
    for (const l of [same, back]) {
      expect(l !== undefined && points(l.d).every(([x]) => x >= 0 && x <= G.days * G.col)).toBe(
        true,
      );
    }
  });

  it('connects milestones at the diamond, not at the column edge', () => {
    const { links } = ganttLinks(
      [row('m', 3, 3, [], true), row('t', 6, 7, ['m']), row('n', 9, 9, ['t'], true)],
      G,
    );
    // ◆ 가운데 3*2+1=7, 꼭짓점 밖 0.55 → 7.55에서 나간다.
    expect(links.find((l) => l.key === 'm>t')?.d.startsWith('M7.55 0.875')).toBe(true);
    // t의 끝(열 7)에서 n의 ◆ 왼쪽 꼭짓점 앞(9*2+1-0.55=18.45)으로 들어간다.
    const into = links.find((l) => l.key === 't>n');
    expect(into !== undefined && rightAngledInto(into.d, [18.45, 4.375])).toBe(true);
  });

  it('draws nothing to an end that is not on the chart, and counts it once', () => {
    // 'gone'은 거르기로 사라졌거나 볼 수 없는 업무, 'u'는 날짜 없음(그리지 않은 업무 목록에만 있음).
    const all = [
      { id: 'a', predecessor_ids: [] },
      { id: 'b', predecessor_ids: ['a', 'gone'] },
      { id: 'u', predecessor_ids: ['a'] },
    ];
    const { links, hidden } = ganttLinks([row('a', 0, 1), row('b', 4, 5, ['a', 'gone'])], G, all);
    expect(links.map((l) => l.key)).toEqual(['a>b']);
    expect(hidden).toBe(2);
  });

  it('does not start a line from a bar cut off at the end of the shown period', () => {
    const base = { status: 'todo' as const, parent_id: null, milestone: false };
    const layout = ganttLayout([
      { ...base, id: 'a', start_date: '2026-01-01', due_date: '2027-06-01', predecessor_ids: [] },
      {
        ...base,
        id: 'b',
        start_date: '2026-03-01',
        due_date: '2026-03-02',
        predecessor_ids: ['a'],
      },
      { ...base, id: 'c', start_date: '2026-01-02', due_date: '2026-01-03', predecessor_ids: [] },
    ]);
    expect(layout.rows.find((r) => r.task.id === 'a')?.clippedEnd).toBe(true);
    expect(layout.days).toBe(MAX_GANTT_DAYS);
    const { links, hidden } = ganttLinks(layout.rows, { col: 1.75, row: 1.75, days: layout.days });
    expect(links).toEqual([]);
    expect(hidden).toBe(1);
  });

  it('survives odd data: self links, unknown ids, missing lists', () => {
    const odd = {
      task: { id: 'x', milestone: false, predecessor_ids: ['x', 'nope'] },
      from: 0,
      to: 0,
    };
    // 응답에서 목록이 빠진 경우(형식상 있어야 하지만 실제로 없을 때).
    const broken = row('y', 1, 1);
    Reflect.deleteProperty(broken.task, 'predecessor_ids');
    const { links, hidden } = ganttLinks([odd, broken], G);
    expect(links).toEqual([]);
    expect(hidden).toBe(1);
  });

  it('follows the row order and the column width (sort, resize, zoom)', () => {
    const rows = [row('a', 0, 1), row('b', 3, 4, ['a'])];
    const before = ganttLinks(rows, G).links[0];
    // 정렬로 줄 순서가 바뀌면 y가, 열이 넓어지면 x가 막대와 같은 비율로 바뀐다.
    const swapped = ganttLinks([...rows].reverse(), G).links[0];
    const wider = ganttLinks(rows, { ...G, col: 4 }).links[0];
    expect(before?.d).toBe('M3.875 0.875H4.275V2.625H6.125');
    expect(swapped?.d).toBe('M3.875 2.625H4.275V0.875H6.125');
    expect(wider?.d).toBe('M7.875 0.875H8.275V2.625H12.125');
  });
});
