import { beforeEach, describe, expect, it } from 'vitest';

import { parseRevision } from '$lib/api/client';
import { blockLines, diffRevisions } from '$lib/features/documents/revision-diff';
import { setLocale } from '$lib/paraglide/runtime.js';

const p = (text: string) => ({ type: 'paragraph', content: [{ type: 'text', text }] });

describe('revision diff', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });

  it('flattens blocks into readable lines', () => {
    const body = {
      type: 'doc',
      content: [
        { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: '목표' }] },
        p('본문'),
        {
          type: 'bulletList',
          content: [
            { type: 'listItem', content: [p('하나')] },
            {
              type: 'listItem',
              content: [
                p('둘'),
                { type: 'bulletList', content: [{ type: 'listItem', content: [p('둘의 하위')] }] },
              ],
            },
          ],
        },
        { type: 'paragraph' },
      ],
    };
    expect(blockLines(body)).toEqual(['## 목표', '본문', '- 하나', '- 둘', '  - 둘의 하위']);
  });

  it('marks added and removed lines including the title', () => {
    const before = { title: '계획', body: { type: 'doc', content: [p('유지'), p('삭제될 줄')] } };
    const after = { title: '계획 v2', body: { type: 'doc', content: [p('유지'), p('새 줄')] } };
    const lines = diffRevisions(before, after);
    expect(lines.filter((l) => l.kind === 'removed').map((l) => l.text)).toEqual([
      '제목: 계획',
      '삭제될 줄',
    ]);
    expect(lines.filter((l) => l.kind === 'added').map((l) => l.text)).toEqual([
      '제목: 계획 v2',
      '새 줄',
    ]);
    expect(lines.find((l) => l.text === '유지')?.kind).toBe('same');
  });

  it('parses revisions with an optional restore source', () => {
    const r = parseRevision({
      revision: 5,
      title: 't',
      author_name: null,
      created_at: '2026-10-17T00:00:00Z',
      restored_from: 2,
      body: { type: 'doc' },
      schema_version: 1,
    });
    expect(r.restored_from).toBe(2);
    expect(r.author_name).toBeNull();
  });
});

describe('revision diff for checklists', () => {
  it('shows checked state so a toggle is a visible change', () => {
    const item = (checked: boolean, text: string) => ({
      type: 'taskItem',
      attrs: { checked },
      content: [{ type: 'paragraph', content: [{ type: 'text', text }] }],
    });
    const before = { type: 'doc', content: [{ type: 'taskList', content: [item(false, '초안')] }] };
    const after = { type: 'doc', content: [{ type: 'taskList', content: [item(true, '초안')] }] };
    expect(blockLines(before)).toEqual(['- [ ] 초안']);
    const lines = diffRevisions({ title: 't', body: before }, { title: 't', body: after });
    expect(lines.filter((l) => l.kind !== 'same').map((l) => `${l.kind}:${l.text}`)).toEqual([
      'removed:- [ ] 초안',
      'added:- [x] 초안',
    ]);
  });
});
