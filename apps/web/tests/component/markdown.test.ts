import { Editor } from '@tiptap/core';
import { TaskItem, TaskList } from '@tiptap/extension-list';
import { Markdown } from '@tiptap/markdown';
import StarterKit from '@tiptap/starter-kit';
import { describe, expect, it } from 'vitest';

import { canonicalJson } from '$lib/features/documents/drafts';
import { Reference } from '$lib/features/references/reference-extension';

const id = '0198f1f2-0000-7000-8000-000000000004';

function editorWith(content: object): Editor {
  return new Editor({
    extensions: [StarterKit, TaskList, TaskItem.configure({ nested: true }), Markdown, Reference],
    content,
  });
}

describe('markdown mode (D-27)', () => {
  it('round-trips references and checklists without loss', () => {
    const body = {
      type: 'doc',
      content: [
        {
          type: 'paragraph',
          content: [
            { type: 'text', text: '참고 ' },
            { type: 'reference', attrs: { kind: 'item', id } },
          ],
        },
        {
          type: 'taskList',
          content: [
            {
              type: 'taskItem',
              attrs: { checked: true },
              content: [{ type: 'paragraph', content: [{ type: 'text', text: '자료 수집' }] }],
            },
            {
              type: 'taskItem',
              attrs: { checked: false },
              content: [{ type: 'paragraph', content: [{ type: 'text', text: '초안 작성' }] }],
            },
          ],
        },
      ],
    };
    const editor = editorWith(body);
    const md = editor.getMarkdown();
    expect(md).toContain(`[reference kind="item" id="${id}"]`);
    expect(md).toContain('- [x] 자료 수집');
    expect(md).toContain('- [ ] 초안 작성');
    const parsed = editor.markdown?.parse(md);
    expect(canonicalJson(parsed)).toBe(canonicalJson(editor.getJSON()));
    editor.destroy();
  });

  it('imports pasted markdown checklists with their states (D-30)', () => {
    const editor = editorWith({ type: 'doc', content: [{ type: 'paragraph' }] });
    const parsed = editor.markdown?.parse('- [x] 자료 수집\n- [ ] 초안 작성\n');
    const items = (parsed?.content?.[0]?.content ?? []).map((item): unknown[] => {
      const checked: unknown = item.attrs?.checked;
      return [checked, item.content?.[0]?.content?.[0]?.text];
    });
    expect(items).toEqual([
      [true, '자료 수집'],
      [false, '초안 작성'],
    ]);
    editor.destroy();
  });
});
