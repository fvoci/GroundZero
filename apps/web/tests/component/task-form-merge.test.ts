import { describe, expect, it } from 'vitest';

import type { TaskInput } from '$lib/api/client';
import { estimateOf, mergeEstimate, mergeForm } from '$lib/features/tasks/form';

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
    // 마감일은 같은 값으로 바꿨고 마일스톤은 서버에서만 바꿨다.
    const mine = { ...base, title: '내 제목', due_date: '2026-11-01', tags: ['내 태그'] };
    const fresh = {
      ...base,
      title: '남의 제목',
      due_date: '2026-11-01',
      tags: ['서버'],
      milestone: true,
    };
    expect(mergeForm(base, mine, fresh).clashes).toEqual(['title', 'tags']);
    expect(mergeForm(base, base, fresh).clashes).toEqual([]);
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
