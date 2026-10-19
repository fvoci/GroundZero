import { describe, expect, it } from 'vitest';

import type { TaskInput } from '$lib/api/client';
import { mergeForm } from '$lib/features/tasks/form';

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
