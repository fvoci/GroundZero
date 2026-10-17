import { describe, expect, it } from 'vitest';

import { parseTasksCsv } from '$lib/features/tasks/csv';

const people = [
  { user_id: 'u1', display_name: '김철수' },
  { user_id: 'u2', display_name: '이영희' },
];

describe('task CSV', () => {
  it('reads fields, maps Korean values and assignees by name', () => {
    const r = parseTasksCsv(
      '제목,설명,상태,우선순위,시작일,기한,예상,담당자,태그\n' +
        '"로그인, 화면",설계 포함,진행 중,높음,2026-10-20,2026-10-25,90,김철수;이영희,ui;auth\n' +
        '배포,,,,,,,,\n',
      people,
    );
    expect(r.errors).toEqual([]);
    expect(r.rows[0]).toEqual({
      title: '로그인, 화면',
      description: '설계 포함',
      status: 'in_progress',
      priority: 'high',
      start_date: '2026-10-20',
      due_date: '2026-10-25',
      estimate_minutes: 90,
      assignee_ids: ['u1', 'u2'],
      tags: ['ui', 'auth'],
    });
    expect(r.rows[1]?.status).toBe('todo');
    expect(r.rows[1]?.priority).toBe('none');
  });

  it('reports each bad line by number', () => {
    const r = parseTasksCsv(
      ',설명만\n일,,끝남\n일,,,급함\n일,,,,2026-10-30,2026-10-01\n일,,,,,,오래\n일,,,,,,,박민수',
      people,
    );
    expect(r.errors.map((e) => e.line)).toEqual([1, 2, 3, 4, 5, 6]);
    expect(r.rows).toEqual([]);
  });

  it('skips rows of empty cells and reports a long title on its own line', () => {
    const r = parseTasksCsv(`,,,\n"둘째\n줄",,\n${'가'.repeat(201)}`, people);
    expect(r.rows.map((x) => x.title)).toEqual(['둘째\n줄']);
    expect(r.errors).toEqual([{ line: 4, reason: '제목은 200자까지입니다' }]);
  });
});
