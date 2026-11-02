import { describe, expect, it } from 'vitest';
import {
  fieldDifferences,
  initialFieldChoices,
  selectedFields,
} from '$lib/features/common/field-conflict';
import { taskConflictFields } from '$lib/features/tasks/field-conflict';
import type { TaskInput } from '$lib/features/tasks/model';
import { changedTaskFields } from '$lib/features/tasks/form';

const base: TaskInput = {
  title: 'base',
  description: '',
  status: '할 일',
  priority: 'none',
  start_date: null,
  due_date: null,
  milestone: false,
  estimate_minutes: null,
  tags: [],
  assignee_ids: [],
};
const fields = taskConflictFields(
  new Map([
    ['u1', 'Alex #1111'],
    ['u2', 'Alex #2222'],
  ]),
);

describe('field choices (#180)', () => {
  it('sends only changed fields and keeps the full period group together', () => {
    expect(changedTaskFields(base, { ...base, title: 'new' })).toEqual({ title: 'new' });
    expect(changedTaskFields(base, { ...base, due_date: '2026-11-20' })).toEqual({
      start_date: null,
      due_date: '2026-11-20',
      milestone: false,
    });
    expect(changedTaskFields({ ...base, tags: ['a', 'b'] }, { ...base, tags: ['b', 'a'] })).toEqual(
      {},
    );
  });
  it('shows individual tag boundaries even when a tag contains a comma', () => {
    const tags = fields.find((field) => field.key === 'tags');
    expect(tags?.text({ ...base, tags: ['a, b'] })).toBe('"a, b"');
    expect(tags?.text({ ...base, tags: ['a', 'b'] })).toBe('"a", "b"');
  });
  it('sorts both-side changes first, omits agreement and distinguishes server-only changes', () => {
    const mine = { ...base, description: 'mine', priority: 'high' as const };
    const server = { ...base, title: 'server', description: 'server', tags: ['one'] };
    const rows = fieldDifferences(fields, base, mine, server);
    expect(rows.map((r) => [r.field.key, r.kind])).toEqual([
      ['description', 'both'],
      ['title', 'server'],
      ['priority', 'mine'],
      ['tags', 'server'],
    ]);
    expect(initialFieldChoices(rows)).toEqual({
      title: 'server',
      priority: 'mine',
      tags: 'server',
    });
    expect(fieldDifferences(fields, base, mine, mine)).toEqual([]);
  });
  it('keeps the entire period from one side, including milestone and cleared dates', () => {
    const mine = { ...base, start_date: '2026-11-01', due_date: '2026-11-05' };
    const server = { ...base, milestone: true, due_date: '2026-11-20' };
    expect(fieldDifferences(fields, base, mine, server).map((r) => r.field.key)).toEqual([
      'schedule',
    ]);
    expect(selectedFields(fields, mine, server, { schedule: 'mine' })).toMatchObject({
      start_date: '2026-11-01',
      due_date: '2026-11-05',
      milestone: false,
    });
    expect(selectedFields(fields, mine, server, { schedule: 'server' })).toMatchObject({
      start_date: null,
      due_date: '2026-11-20',
      milestone: true,
    });
  });
  it('compares whole assignee and tag sets, without silently joining either set', () => {
    const mine = { ...base, tags: ['a', 'b', 'a'], assignee_ids: ['u2', 'u1'] };
    const server = { ...base, tags: ['b', 'a'], assignee_ids: ['u1', 'u2'] };
    expect(fieldDifferences(fields, base, mine, server)).toEqual([]);
    const different = { ...server, tags: ['c'], assignee_ids: ['u3'] };
    expect(selectedFields(fields, mine, different, { tags: 'mine' }).assignee_ids).toEqual(['u3']);
    expect(selectedFields(fields, mine, different, { assignee_ids: 'mine' }).tags).toEqual(['c']);
  });
  it('normalizes surrounding whitespace while keeping internal, invisible and long text differences', () => {
    expect(fieldDifferences(fields, base, { ...base, title: ' base\u0085' }, base)).toEqual([]);
    const mine = { ...base, description: `first\n${'x'.repeat(5000)}\nlast  line\uFEFF` };
    const server = { ...mine, description: mine.description.replace('  ', ' ') };
    const rows = fieldDifferences(fields, base, mine, server);
    expect(rows).toHaveLength(1);
    expect(rows[0]?.field.text(mine)).toBe(mine.description);
    expect(rows[0]?.field.text(server)).toBe(server.description);
  });
  it('distinguishes zero estimate from an empty estimate', () => {
    expect(
      selectedFields(fields, { ...base, estimate_minutes: 0 }, base, { estimate_minutes: 'mine' })
        .estimate_minutes,
    ).toBe(0);
    expect(
      selectedFields(fields, base, { ...base, estimate_minutes: 15 }, { estimate_minutes: 'mine' })
        .estimate_minutes,
    ).toBeNull();
  });
});
