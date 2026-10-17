import { describe, expect, it } from 'vitest';

import type { RecordKindView, RecordView } from '$lib/api/client';
import { emptyForm, formOf, summary, toInput } from '$lib/features/records/records';

const def: RecordKindView = {
  kind: 'analysis',
  label: '분석',
  module_kind: 'research',
  date_label: '실행일',
  fields: [
    { key: 'status', label: '상태', type: 'choice', choices: [{ value: 'done', label: '완료' }] },
    { key: 'owner', label: '담당', type: 'person', choices: [] },
    { key: 'team', label: '참여자', type: 'people', choices: [] },
    { key: 'minutes', label: '시간', type: 'number', choices: [] },
    { key: 'tags', label: '태그', type: 'tags', choices: [] },
    { key: 'url', label: '주소', type: 'url', choices: [] },
  ],
  file_roles: [{ value: 'input', label: '입력 데이터' }],
};

describe('record forms', () => {
  it('builds input from the form, dropping blanks and checking numbers', () => {
    const form = emptyForm();
    form.title = ' 회귀 ';
    form.occurred_on = '2026-10-17';
    form.values = { status: 'done', owner: 'u1', minutes: '12.5', tags: 'a, b, a,', url: ' ' };
    form.people = { team: ['u1', 'u2'] };
    form.files = [
      { file_id: 'f1', version: '', role: 'input' },
      { file_id: '', version: '', role: 'input' },
    ];
    const built = toInput(def, form);
    expect(built).toEqual({
      input: {
        title: '회귀',
        occurred_on: '2026-10-17',
        body: '',
        fields: {
          status: 'done',
          owner: 'u1',
          team: ['u1', 'u2'],
          minutes: 12.5,
          tags: ['a', 'b'],
        },
        files: [{ file_id: 'f1', version: null, role: 'input' }],
      },
    });
    form.values = { minutes: '-1' };
    expect(toInput(def, form)).toEqual({ error: '시간에는 0 이상의 수를 적어 주세요.' });
  });

  it('round-trips a record and summarizes choices, people and links', () => {
    const r: RecordView = {
      id: 'r1',
      kind: 'analysis',
      title: '회귀',
      occurred_on: null,
      body: '',
      fields: { status: 'done', owner: 'u9', team: ['u1'], tags: ['a'], url: 'https://x.test' },
      files: [
        {
          role: 'input',
          file_id: null,
          file_name: 'gone.csv',
          version: 2,
          sha256: 'ab',
          size: 1,
          available: false,
        },
      ],
      created_by: 'u1',
      created_by_name: null,
      updated_at: '2026-10-17T00:00:00Z',
      version: 1,
      can_edit: true,
      can_delete: true,
    };
    const form = formOf(def, r);
    expect(form.values.tags).toBe('a');
    expect(form.people.team).toEqual(['u1']);
    expect(form.files).toEqual([]);
    const lines = summary(def, r, new Map([['u1', '김']]), new Map());
    expect(lines.map((l) => [l.label, l.text])).toEqual([
      ['상태', '완료'],
      ['담당', '알 수 없는 사람'],
      ['참여자', '김'],
      ['태그', 'a'],
      ['주소', 'https://x.test'],
    ]);
    expect(lines.at(-1)?.href).toBe('https://x.test');
  });
});
