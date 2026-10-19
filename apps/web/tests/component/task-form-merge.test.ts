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
