import { describe, expect, it } from 'vitest';

import type { EventInput, EventView } from '$lib/api/client';
import { mergeEventInput } from '$lib/features/events/conflict';

// D-94: 충돌 뒤 "내 입력으로 다시 저장"은 내가 고친 칸만 서버 최신 값에 얹는다. 같은 칸을 양쪽이 서로 다른 값으로
// 고쳤으면 겹친 칸으로 돌려준다. 날짜·시각·시간대·종일 여부는 한 묶음이다.
function ev(fields: Partial<EventInput> = {}, version = 5): EventView {
  return {
    id: 'A',
    item_id: 'i1',
    workspace_id: 'ws-1',
    kind: 'meeting',
    title: '회의',
    description: '',
    all_day: false,
    start_date: null,
    end_date: null,
    starts_at: '2026-10-20T01:00:00Z',
    ends_at: '2026-10-20T02:00:00Z',
    timezone: 'Asia/Seoul',
    recurrence: null,
    excluded_dates: [],
    version,
    access: 'write',
    ...fields,
  };
}

/** 폼이 보내는 입력(입력 칸만). */
function input(fields: Partial<EventInput> = {}): EventInput {
  const e = ev(fields);
  return {
    kind: e.kind,
    title: e.title,
    description: e.description,
    all_day: e.all_day,
    start_date: e.start_date,
    end_date: e.end_date,
    starts_at: e.starts_at,
    ends_at: e.ends_at,
    timezone: e.timezone,
  };
}

describe('mergeEventInput', () => {
  const base = ev();

  it('keeps the server value for fields I did not touch and mine for fields only I changed', () => {
    const server = ev({ title: '새 제목', starts_at: '2026-10-20T05:00:00Z' }, 6);
    const merged = mergeEventInput(base, input({ description: '내 설명' }), server);
    expect(merged.clashes).toEqual([]);
    // 보내는 본문에는 입력 칸만 있다(서버 일정의 id·버전·권한이 섞이지 않음).
    expect(merged.input).toEqual(
      input({ title: '새 제목', starts_at: '2026-10-20T05:00:00Z', description: '내 설명' }),
    );
  });

  it('takes mine without asking when the server changed the field to the same value', () => {
    const server = ev({ title: '같은 제목', kind: 'presentation' }, 6);
    const merged = mergeEventInput(base, input({ title: '같은 제목' }), server);
    expect(merged.clashes).toEqual([]);
    expect(merged.input).toEqual(input({ title: '같은 제목', kind: 'presentation' }));
  });

  it('reports a clash when both changed the same field to different values', () => {
    const server = ev({ title: '그쪽 제목', description: '그쪽 설명' }, 6);
    const merged = mergeEventInput(
      base,
      input({ title: '내 제목', kind: 'event', description: '' }),
      server,
    );
    expect(merged.clashes).toEqual(['title']);
    // 겹친 칸도 내 값으로 둔다(덮어쓰기를 확인받은 뒤에만 보냄). 내가 안 고친 설명은 서버 값.
    expect(merged.input).toEqual(
      input({ title: '내 제목', kind: 'event', description: '그쪽 설명' }),
    );
  });

  it('treats a title or description that differs only by surrounding spaces as untouched', () => {
    // 서버는 제목·설명의 앞뒤 공백을 지워 저장한다. 공백만 덧붙였으면 고친 것이 아니니 서버 값을 두고 묻지 않는다.
    const server = ev({ title: '새 제목', description: '그쪽 설명' }, 6);
    const merged = mergeEventInput(base, input({ title: ' 회의 ', description: '\n' }), server);
    expect(merged.clashes).toEqual([]);
    expect(merged.input).toEqual(input({ title: '새 제목', description: '그쪽 설명' }));
  });

  it('compares instants by value, not by how they are written', () => {
    // 서버가 같은 시각을 다른 표기로 돌려줘도 바뀐 것이 아니다. 내 시각도 소수 초 표기로 같으면 고친 것이 아니다.
    const server = ev(
      { starts_at: '2026-10-20T10:00:00+09:00', ends_at: '2026-10-20T02:00:00.000Z' },
      6,
    );
    const merged = mergeEventInput(
      base,
      input({ starts_at: '2026-10-20T01:00:00.000Z', title: '내 제목' }),
      server,
    );
    expect(merged.clashes).toEqual([]);
    expect(merged.input.starts_at).toBe('2026-10-20T10:00:00+09:00');
    expect(merged.input.title).toBe('내 제목');
  });

  it('treats date, time, time zone and all-day as one unit', () => {
    // 나는 시작 시각을, 서버는 시간대를 바꿨다: 시작만 내 것, 시간대는 서버 것으로 섞지 않고 겹친 칸으로 묻는다.
    const server = ev({ timezone: 'Asia/Tokyo' }, 6);
    const mine = input({ starts_at: '2026-10-20T00:30:00.000Z' });
    const merged = mergeEventInput(base, mine, server);
    expect(merged.clashes).toEqual(['when']);
    expect(merged.input).toEqual(mine);
  });

  it('carries the whole unit when I switch to an all-day event the server left alone', () => {
    const server = ev({ title: '새 제목' }, 6);
    const mine = input({
      all_day: true,
      start_date: '2026-10-20',
      end_date: '2026-10-21',
      starts_at: null,
      ends_at: null,
    });
    const merged = mergeEventInput(base, mine, server);
    expect(merged.clashes).toEqual([]);
    expect(merged.input).toEqual({ ...mine, title: '새 제목' });
  });

  it('keeps the server unit whole when only the server changed the time', () => {
    const server = ev(
      {
        all_day: true,
        start_date: '2026-10-22',
        end_date: '2026-10-22',
        starts_at: null,
        ends_at: null,
      },
      6,
    );
    const merged = mergeEventInput(base, input({ kind: 'event' }), server);
    expect(merged.clashes).toEqual([]);
    expect(merged.input).toEqual(
      input({
        kind: 'event',
        all_day: true,
        start_date: '2026-10-22',
        end_date: '2026-10-22',
        starts_at: null,
        ends_at: null,
      }),
    );
  });
});
