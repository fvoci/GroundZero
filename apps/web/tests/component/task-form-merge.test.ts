import { describe, expect, it } from 'vitest';

import type { TaskInput } from '$lib/api/client';
import { formatDay } from '$lib/display';
import type { MergeUnit } from '$lib/features/tasks/form';
import { estimateOf, mergeEstimate, mergeForm } from '$lib/features/tasks/form';
import { assigneeLabels, clashRows } from '$lib/features/tasks/labels';
import { setLocale } from '$lib/paraglide/runtime.js';

const base: TaskInput = {
  title: '보고서',
  description: '',
  status: 'todo',
  priority: 'none',
  start_date: null,
  due_date: null,
  estimate_minutes: null,
  tags: [],
  assignee_ids: [],
  milestone: false,
};

// 체크리스트·반복을 바꾼 뒤 다시 채울 때, 저장하지 않은 입력을 덮지 않는다(제품 로직 점검 1·8번 불변식).
describe('mergeForm', () => {
  it('keeps unsaved edits and takes the server value for untouched fields', () => {
    const mine = { ...base, title: '고친 제목' };
    const fresh = { ...base, due_date: '2026-10-30' };
    const merged = mergeForm(base, mine, fresh);
    expect(merged.form.title).toBe('고친 제목');
    expect(merged.form.due_date).toBe('2026-10-30');
    expect(merged.clash).toBe(false);
  });

  it('reports a clash when the same field changed on both sides', () => {
    const merged = mergeForm(base, { ...base, title: '내 제목' }, { ...base, title: '남의 제목' });
    expect(merged.form.title).toBe('내 제목');
    expect(merged.clash).toBe(true);
    expect(mergeForm(base, { ...base, title: '같음' }, { ...base, title: '같음' }).clash).toBe(
      false,
    );
  });

  // 충돌 화면에서 덮어쓸지 물을 칸(D-94): 양쪽이 서로 다르게 바꾼 칸만, 폼 순서로.
  it('names the fields both sides changed to different values', () => {
    // 마감일은 같은 값으로 바꿨지만 서버는 마일스톤도 바꿨다. 일정(시작일·마감일·마일스톤)은 한 묶음이라 양쪽 일정이
    // 다르므로 묻는다(리뷰 전에는 마감일만 견주어 묻지 않고 서버의 마일스톤과 섞었다).
    const mine = { ...base, title: '내 제목', due_date: '2026-11-01', tags: ['내 태그'] };
    const fresh = {
      ...base,
      title: '남의 제목',
      due_date: '2026-11-01',
      tags: ['서버'],
      milestone: true,
    };
    expect(mergeForm(base, mine, fresh).clashes).toEqual(['title', 'schedule', 'tags']);
    expect(mergeForm(base, base, fresh).clashes).toEqual([]);
  });
});

// 일정(시작일·마감일·마일스톤)은 한 단위다(D-94의 일정 묶음을 업무 일정에 적용한 해석, 리뷰 1). 칸마다 고르면 아무도
// 고르지 않은 조합이 저장되거나 한 줄로 보였다.
describe('mergeForm schedule unit', () => {
  const dated: TaskInput = { ...base, start_date: '2026-10-01', due_date: '2026-10-10' };
  const scheduleOf = (input: TaskInput) => ({
    start_date: input.start_date,
    due_date: input.due_date,
    milestone: input.milestone,
  });

  it('does not join the server start with my due date', () => {
    // 서버는 시작일을 10-05로, 나는 마감일을 10-03으로 바꿨다. 칸마다 고르면 10-05 ~ 10-03(서버가 400으로 거절).
    const fresh = { ...dated, start_date: '2026-10-05' };
    const mine = { ...dated, due_date: '2026-10-03' };
    const merged = mergeForm(dated, mine, fresh);
    expect(merged.clashes).toEqual(['schedule']);
    expect(scheduleOf(merged.form)).toEqual({
      start_date: '2026-10-01',
      due_date: '2026-10-03',
      milestone: false,
    });
  });

  it('does not join my start with the server due date', () => {
    // 서버 10-05 ~ 10-15, 내 시작일 10-03. 칸마다 고르면 10-03 ~ 10-15를 내 값처럼 한 줄로 보였다.
    const fresh = { ...dated, start_date: '2026-10-05', due_date: '2026-10-15' };
    const mine = { ...dated, start_date: '2026-10-03' };
    const merged = mergeForm(dated, mine, fresh);
    expect(merged.clashes).toEqual(['schedule']);
    expect(scheduleOf(merged.form)).toEqual({
      start_date: '2026-10-03',
      due_date: '2026-10-10',
      milestone: false,
    });
    // 기준은 옛 일정 전체로 남아 다음 다시 채우기에서도 충돌로 본다.
    expect(scheduleOf(merged.baseline)).toEqual(scheduleOf(dated));
  });

  it('takes the whole server schedule when I did not touch it', () => {
    const fresh = { ...dated, start_date: null, due_date: '2026-10-15', milestone: true };
    const merged = mergeForm(dated, { ...dated, title: '내 제목' }, fresh);
    expect(merged.clashes).toEqual([]);
    expect(scheduleOf(merged.form)).toEqual(scheduleOf(fresh));
    expect(scheduleOf(merged.baseline)).toEqual(scheduleOf(fresh));
  });

  it('keeps my whole schedule when the server schedule did not change', () => {
    const mine = { ...dated, start_date: null, milestone: true };
    const merged = mergeForm(dated, mine, { ...dated, title: '남의 제목' });
    expect(merged.clashes).toEqual([]);
    expect(scheduleOf(merged.form)).toEqual({
      start_date: null,
      due_date: '2026-10-10',
      milestone: true,
    });
    expect(merged.form.title).toBe('남의 제목');
  });

  it('does not ask when both sides chose the same schedule', () => {
    const same = { ...dated, start_date: '2026-10-02', due_date: '2026-10-12' };
    expect(mergeForm(dated, same, same).clashes).toEqual([]);
  });

  // 다시 채우기(체크리스트·반복·선행 업무를 바꾼 뒤)도 같은 병합을 쓴다: 나는 시작일만, 서버는 마감일만 바꿨으면
  // 섞지 않고 내 일정 전체를 지키며 충돌로 남긴다(저장하면 옛 판 번호로 보내 충돌 화면이 고르게 한다).
  it('keeps a refresh from mixing my start with the server due date', () => {
    const mine = { ...dated, start_date: '2026-10-02' };
    const fresh = { ...dated, due_date: '2026-10-20' };
    const merged = mergeForm(dated, mine, fresh);
    expect(merged.clash).toBe(true);
    expect(scheduleOf(merged.form)).toEqual(scheduleOf(mine));
  });

  // 서버가 마일스톤으로 바꿨을 때 내 시작일을 견주면 마일스톤이 섞여 시작일이 사라졌다(리뷰 2).
  it('keeps my dates whole when the server made it a milestone', () => {
    const fresh = { ...dated, start_date: null, milestone: true };
    const mine = { ...dated, start_date: '2026-10-03' };
    const merged = mergeForm(dated, mine, fresh);
    expect(merged.clashes).toEqual(['schedule']);
    expect(scheduleOf(merged.form)).toEqual({
      start_date: '2026-10-03',
      due_date: '2026-10-10',
      milestone: false,
    });
  });
});

// 담당자는 순서 없는 모음이다. 서버는 이름순으로 돌려주고 화면은 고른 순서로 덧붙여, 같은 사람들을 순서만 달리 보고
// 충돌로 묻거나 다시 채운 뒤 쓸데없이 옛 판 번호로 보내 409를 만들었다(리뷰 3).
describe('mergeForm assignees and text', () => {
  it('compares assignees as a set', () => {
    const before = { ...base, assignee_ids: ['u2'] };
    const mine = { ...base, assignee_ids: ['u2', 'u1'] };
    const fresh = { ...base, assignee_ids: ['u1', 'u2'] };
    const merged = mergeForm(before, mine, fresh);
    expect(merged.clash).toBe(false);
    expect(merged.form.assignee_ids).toEqual(['u1', 'u2']);
    // 순서만 바꾼 것은 고친 것이 아니므로 서버가 바꾼 담당자를 따른다.
    const reordered = mergeForm(
      { ...base, assignee_ids: ['u1', 'u2'] },
      { ...base, assignee_ids: ['u2', 'u1'] },
      { ...base, assignee_ids: ['u3'] },
    );
    expect(reordered.clash).toBe(false);
    expect(reordered.form.assignee_ids).toEqual(['u3']);
  });

  // 태그는 서버가 앞뒤 공백을 떼고 겹친 것을 빼지만 받은 순서는 지킨다(tasks.rs validate_tags). 순서도 값이다.
  it('keeps comparing tags in order', () => {
    expect(
      mergeForm(base, { ...base, tags: ['a', 'b'] }, { ...base, tags: ['b', 'a'] }).clashes,
    ).toEqual(['tags']);
  });

  // 제목·설명은 서버가 앞뒤 공백을 떼고 저장한다: 서버 값과 앞뒤 공백만 다른 내 값은 겹친 것이 아니다(묻는 화면에서 두
  // 값이 똑같아 보인다). 친 글은 폼에 그대로 두고, 기준은 새 값으로 옮긴다.
  it('does not ask when my text differs from the server only by surrounding spaces', () => {
    const merged = mergeForm(
      base,
      { ...base, description: '새 설명\n' },
      { ...base, description: '새 설명' },
    );
    expect(merged.clash).toBe(false);
    expect(merged.form.description).toBe('새 설명\n');
    expect(merged.baseline.description).toBe('새 설명');
  });

  // 다시 채우기는 앞뒤 공백만 친 중인 글도 고친 칸으로 지킨다(뒤 공백을 지워 다음 낱말이 붙지 않게).
  it('keeps text being typed with a trailing space on refresh', () => {
    const merged = mergeForm(base, { ...base, title: `${base.title} ` }, base);
    expect(merged.form.title).toBe(`${base.title} `);
  });
});

describe('mergeForm baseline', () => {
  it('keeps a clash visible across a second refresh and catches tag clashes', () => {
    const mine = { ...base, title: '내 제목', tags: ['내 태그'] };
    const first = mergeForm(base, mine, { ...base, title: '남의 제목', tags: ['남의 태그'] });
    expect(first.clash).toBe(true);
    // 사용자가 바꾼 칸은 기준이 옛 값 그대로라, 다음 새로고침에서도 서버의 바뀐 값과 다르다고 본다.
    expect(first.baseline.title).toBe(base.title);
    const second = mergeForm(first.baseline, first.form, {
      ...base,
      title: '남의 제목',
      tags: ['남의 태그'],
      due_date: '2026-11-01',
    });
    expect(second.clash).toBe(true);
    expect(second.form.title).toBe('내 제목');
    expect(second.form.due_date).toBe('2026-11-01');
  });

  // 기준 A, 내 입력과 서버가 모두 B → 기준도 B. 그 뒤 내가 D로 고치고 서버는 그대로 B면 충돌이 아니다.
  it('moves the baseline when my value already matches the server', () => {
    const first = mergeForm(base, { ...base, title: 'B' }, { ...base, title: 'B' });
    expect(first.baseline.title).toBe('B');
    const second = mergeForm(
      first.baseline,
      { ...first.form, title: 'D' },
      { ...base, title: 'B' },
    );
    expect(second.clash).toBe(false);
    expect(second.form.title).toBe('D');
  });

  // 기준 A, 내 B, 서버 C로 충돌한 뒤 내가 C로 맞추면 다음 다시 채우기에서는 충돌이 아니다(업무 상세는 이 결과로 충돌을
  // 다시 판단해 최신 판으로 저장한다).
  it('releases a clash once my value matches the server', () => {
    const first = mergeForm(base, { ...base, title: 'B' }, { ...base, title: 'C' });
    expect(first.clash).toBe(true);
    const second = mergeForm(
      first.baseline,
      { ...first.form, title: 'C' },
      { ...base, title: 'C' },
    );
    expect(second.clash).toBe(false);
  });
});

// 예상 시간은 글자로 입력한다. 화면에 남길지와 병합 기준이 같은 판정을 써야 한다(리뷰: "01"을 남긴 채 기준만 서버 값으로
// 옮겨, 저장하면 서버의 새 값 2를 충돌 없이 1로 덮었다).
describe('mergeEstimate', () => {
  it('reads only whole numbers and empty text', () => {
    expect(estimateOf(' 01 ')).toBe(1);
    expect(estimateOf('')).toBeNull();
    expect(estimateOf('1.5')).toBe('invalid');
    expect(estimateOf('1abc')).toBe('invalid');
    expect(estimateOf('-3')).toBe('invalid');
  });

  it('takes the server value when the typed value equals the baseline', () => {
    expect(mergeEstimate(1, '01', 2)).toEqual({ keepText: false, baseline: 2, clash: false });
  });

  it('keeps a changed value and reports a clash when the server also changed it', () => {
    expect(mergeEstimate(1, '5', 2)).toEqual({ keepText: true, baseline: 1, clash: true });
    expect(mergeEstimate(1, '5', 1)).toEqual({ keepText: true, baseline: 1, clash: false });
  });

  it('keeps invalid text as an edit', () => {
    expect(mergeEstimate(null, 'abc', 30)).toEqual({ keepText: true, baseline: null, clash: true });
  });

  it('moves the baseline when the typed value already matches the server', () => {
    expect(mergeEstimate(1, '2', 2)).toEqual({ keepText: false, baseline: 2, clash: false });
  });
});

// 덮어쓰기 확인에 보이는 줄(D-94).
describe('clashRows', () => {
  // 폼의 실제 순서: 제목, 상태, 우선순위, 시작일·마감일(일정 묶음), 설명, 예상 시간, 태그, (마일스톤은 일정 묶음),
  // 담당자(리뷰 7: 전에는 설명을 둘째로 두었다).
  it('lists the units in the order of the form', async () => {
    await setLocale('ko', { reload: false });
    const all: MergeUnit[] = [
      'assignee_ids',
      'tags',
      'estimate_minutes',
      'description',
      'schedule',
      'priority',
      'status',
      'title',
    ];
    expect(clashRows(all, base, base, new Map()).map((row) => row.key)).toEqual([
      'title',
      'status',
      'priority',
      'schedule',
      'description',
      'estimate_minutes',
      'tags',
      'assignee_ids',
    ]);
  });

  // 리뷰 4: 전에는 80자에서 자르고 공백·줄바꿈을 한 칸으로 모아, 서로 다른 값이 똑같이 보였다.
  it('shows full values with their line breaks and spaces', async () => {
    await setLocale('ko', { reload: false });
    const long = '가'.repeat(120);
    const [title, description] = clashRows(
      ['title', 'description'],
      { ...base, title: `${long}A`, description: '첫 줄\n둘째 줄' },
      { ...base, title: `${long}B`, description: '첫 줄 둘째  줄' },
      new Map(),
    );
    expect(title?.server).toBe(`${long}A`);
    expect(title?.mine).toBe(`${long}B`);
    expect(description?.server).toBe('첫 줄\n둘째 줄');
    expect(description?.mine).toBe('첫 줄 둘째  줄');
  });

  it('shows the whole schedule as one row', async () => {
    await setLocale('ko', { reload: false });
    const [row] = clashRows(
      ['schedule'],
      { ...base, start_date: null, due_date: '2026-10-15', milestone: true },
      { ...base, start_date: '2026-10-01', due_date: '2026-10-03' },
      new Map(),
    );
    expect(row?.label).toBe('일정(시작일·마감일·마일스톤)');
    expect(row?.server).toBe(`마일스톤, 마감일 ${formatDay('2026-10-15')}`);
    expect(row?.mine).toBe(`시작일 ${formatDay('2026-10-01')}, 마감일 ${formatDay('2026-10-03')}`);
  });

  // 2차 검증: 서로 다른 값이 똑같아 보이는 쌍은 그렇다고 알린다(`looksSame`).
  it('marks pairs that look the same', async () => {
    await setLocale('ko', { reload: false });
    const looks = (unit: MergeUnit, server: Partial<TaskInput>, mine: Partial<TaskInput>) =>
      clashRows([unit], { ...base, ...server }, { ...base, ...mine }, new Map())[0]?.looksSame;
    expect(looks('description', { description: '가 \n나' }, { description: '가\n나' })).toBe(true);
    // 서버는 U+FEFF를 떼지 않는다(앞에 남아 보이지 않는다).
    expect(looks('title', { title: '﻿제목' }, { title: '제목' })).toBe(true);
    expect(looks('description', { description: '(비어 있음)' }, { description: '' })).toBe(true);
    expect(looks('tags', { tags: ['a, b'] }, { tags: ['a', 'b'] })).toBe(true);
    expect(looks('title', { title: '가  나' }, { title: '가 나' })).toBe(false);
    expect(looks('description', { description: '가\n나' }, { description: '가 나' })).toBe(false);
  });

  // CodeRabbit: 이름이 같은 사람은 id 끝 네 글자로 구분한다(이름이 겹칠 때만, 같은 사람이 여러 번 들어와도 한 사람).
  it('tells apart people with the same name', () => {
    const labels = assigneeLabels([
      { user_id: '0190a000-0000-7000-8000-00000000a1a1', display_name: 'Alex' },
      { user_id: '0190a000-0000-7000-8000-00000000b2b2', display_name: 'Alex' },
      { user_id: '0190a000-0000-7000-8000-00000000a1a1', display_name: 'Alex' },
      { user_id: 'u3', display_name: '보라' },
      { user_id: 'u3', display_name: '보라' },
    ]);
    expect([...labels.values()]).toEqual(['Alex #a1a1', 'Alex #b2b2', '보라']);
  });
});

// 2차 반박 검증: 앞뒤 공백만 친 글은 저장되는 값이 그대로라 고친 칸이 아니다(D-94: 고치지 않은 칸은 서버 값). 공백은
// 서버(Rust `str::trim`, 유니코드 White_Space)와 같은 글자만 뗀다.
describe('mergeForm compares text the way the server trims it', () => {
  it('takes the server value when I only added surrounding spaces', () => {
    const merged = mergeForm(
      base,
      { ...base, title: ` ${base.title} ` },
      { ...base, title: '남의 제목' },
    );
    expect(merged.clash).toBe(false);
    expect(merged.form.title).toBe('남의 제목');
    expect(merged.baseline.title).toBe('남의 제목');
  });

  it('trims the same characters as the server', () => {
    const fresh = { ...base, title: '남의 제목' };
    expect(mergeForm(base, { ...base, title: '남의 제목　' }, fresh).clash).toBe(false);
    // JS `trim`은 U+FEFF를 떼지만 서버는 그대로 저장한다: 서버 값과 다른 값이다.
    expect(mergeForm(base, { ...base, title: '﻿남의 제목' }, fresh).clash).toBe(true);
  });
});

// 2차 검증 nit: 아주 큰 수는 Infinity가 되어 JSON의 null로 보내져 저장된 예상 시간을 지웠다. 서버 범위(i32) 밖은 잘못된
// 입력이다.
describe('estimateOf range', () => {
  it('rejects numbers the server cannot store', () => {
    expect(estimateOf('9'.repeat(309))).toBe('invalid');
    expect(estimateOf('2147483648')).toBe('invalid');
    expect(estimateOf('2147483647')).toBe(2147483647);
  });
});
