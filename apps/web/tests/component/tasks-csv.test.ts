import { beforeEach, describe, expect, it } from 'vitest';
import type { TaskStatusDefinition } from '@fvoci/api-client';

import { parseTasksCsv } from '$lib/features/tasks/csv';
import { setLocale } from '$lib/paraglide/runtime.js';

const people = [
  { user_id: 'u1', display_name: '김철수' },
  { user_id: 'u2', display_name: '이영희' },
];
const statuses: TaskStatusDefinition[] = [
  { id: 'review', name: '검토 대기', color: '', position: 0, category: 'doing', task_count: 0 },
  { id: 'ready', name: 'Ready', color: '', position: 1, category: 'todo', task_count: 0 },
];

describe('task CSV', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });

  it('reads fields, maps Korean values and assignees by name', () => {
    const r = parseTasksCsv(
      '제목,설명,상태,우선순위,시작일,기한,예상,담당자,태그\n' +
        '"로그인, 화면",설계 포함, 검토 대기 ,높음,2026-10-20,2026-10-25,90,김철수;이영희,ui;auth\n' +
        '배포,,,,,,,,\n',
      people,
      statuses,
    );
    expect(r.errors).toEqual([]);
    expect(r.rows[0]).toEqual({
      title: '로그인, 화면',
      description: '설계 포함',
      status: '검토 대기',
      priority: 'high',
      start_date: '2026-10-20',
      due_date: '2026-10-25',
      estimate_minutes: 90,
      assignee_ids: ['u1', 'u2'],
      tags: ['ui', 'auth'],
    });
    expect(r.rows[1]).not.toHaveProperty('status');
    expect(r.rows[1]?.priority).toBe('none');
  });

  it('reports each bad line by number', () => {
    const r = parseTasksCsv(
      ',설명만\n일,,끝남\n일,,,급함\n일,,,,2026-10-30,2026-10-01\n일,,,,,,오래\n일,,,,,,,박민수',
      people,
      statuses,
    );
    expect(r.errors.map((e) => e.line)).toEqual([1, 2, 3, 4, 5, 6]);
    expect(r.rows).toEqual([]);
  });

  // 감사 W3: 서버가 고치기에서 거부할 값(parse_date, validate_tags, validate_text, i32)은 읽을 때 줄 번호로 알린다.
  // 놓치면 만들기(제목만)는 성공하고 고치기가 400이라 제목만 있는 업무가 남는다.
  it('reports values the server would refuse, by line', () => {
    const tags = (n: number): string =>
      Array.from({ length: n }, (_, i) => `t${String(i)}`).join(';');
    const r = parseTasksCsv(
      [
        '없는 날,,,,2026-02-30',
        '이른 해,,,,,1800-01-01',
        '늦은 해,,,,2201-01-01',
        `태그 많음,,,,,,,,${tags(21)}`,
        `긴 태그,,,,,,,,${'가'.repeat(51)}`,
        `긴 설명,${'가'.repeat(10_001)}`,
        '큰 예상,,,,,,2147483648',
        '설명 제어 문자,"a\u0007b"',
        '태그 줄바꿈,,,,,,,,"a\nb"',
      ].join('\n'),
      people,
      statuses,
    );
    expect(r.rows).toEqual([]);
    expect(r.errors).toEqual([
      { line: 1, reason: '날짜는 있는 날짜를 YYYY-MM-DD로 적어 주세요' },
      { line: 2, reason: '날짜는 1900년부터 2200년 사이여야 합니다' },
      { line: 3, reason: '날짜는 1900년부터 2200년 사이여야 합니다' },
      { line: 4, reason: '태그는 20개까지입니다' },
      { line: 5, reason: '태그는 하나에 50자까지입니다' },
      { line: 6, reason: '설명은 10000자까지입니다' },
      { line: 7, reason: '예상 시간(분)이 너무 큽니다: 2147483648' },
      { line: 8, reason: '설명에 쓸 수 없는 제어 문자가 있습니다' },
      { line: 9, reason: '태그에 쓸 수 없는 문자(줄바꿈·탭 등)가 있습니다' },
    ]);
  });

  // 한도 바로 안쪽 값은 받는다. 글자 수는 서버처럼 문자(코드 포인트)로 센다.
  it('accepts values right at the server limits', () => {
    const tags = Array.from(
      { length: 20 },
      (_, i) => `${'태'.repeat(48)}${String(i).padStart(2, '0')}`,
    );
    const r = parseTasksCsv(
      [
        `${'😀'.repeat(200)},"${'😀'.repeat(10_000)}",,,1900-01-01,2200-12-31,2147483647,,${tags.join(';')}`,
        '윤일,"줄\n바꿈\t탭",,,2028-02-29',
      ].join('\n'),
      people,
      statuses,
    );
    expect(r.errors).toEqual([]);
    expect(r.rows[0]?.tags).toHaveLength(20);
    expect(r.rows[0]?.estimate_minutes).toBe(2147483647);
    expect(r.rows[1]?.start_date).toBe('2028-02-29');
  });

  it('skips rows of empty cells and reports a long title on its own line', () => {
    const r = parseTasksCsv(`,,,\n"둘째\n줄",,\n${'가'.repeat(201)}`, people, statuses);
    expect(r.rows.map((x) => x.title)).toEqual(['둘째\n줄']);
    expect(r.errors).toEqual([{ line: 4, reason: '제목은 200자까지입니다' }]);
  });

  it('matches exact names first and accepts only one case-insensitive match', () => {
    expect(parseTasksCsv('업무,, rEaDy ', people, statuses).rows[0]?.status).toBe('Ready');
    const ambiguous = [
      ...statuses,
      {
        id: 'upper',
        name: 'READY',
        color: '',
        position: 2,
        category: 'todo',
        task_count: 0,
      } satisfies TaskStatusDefinition,
    ];
    const result = parseTasksCsv(
      '첫째,,Ready\n둘째,,READY\n셋째,,ready\n넷째,,todo\n다섯째,,완료',
      people,
      ambiguous,
    );
    expect(result.rows.map((row) => row.status)).toEqual(['Ready', 'READY']);
    expect(result.errors).toEqual([
      { line: 3, reason: '상태를 알 수 없습니다: ready' },
      { line: 4, reason: '상태를 알 수 없습니다: todo' },
      { line: 5, reason: '상태를 알 수 없습니다: 완료' },
    ]);
    expect(parseTasksCsv('업무,,   ', people, []).rows[0]).not.toHaveProperty('status');
  });
});
