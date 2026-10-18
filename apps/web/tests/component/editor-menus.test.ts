import { Editor } from '@tiptap/core';
import { Slice } from '@tiptap/pm/model';
import { afterEach, describe, expect, it } from 'vitest';

import { schemaExtensions } from '$lib/features/documents/editor-schema';
import { normalizeHref } from '$lib/features/documents/link-href';
import { cleanSlice, isAllowedHref, isAttachmentUrl } from '$lib/features/documents/paste-guard';
import type { SlashItem } from '$lib/features/documents/slash-command';
import { filterSlashItems, isComposingKey } from '$lib/features/documents/slash-command';

const editors: Editor[] = [];
afterEach(() => {
  for (const e of editors.splice(0)) e.destroy();
});

const FILE = '/api/files/01a14e70-ad23-7648-9f7d-ccb0f85c5b5c/content';

describe('paste guard (same rules as the server document check)', () => {
  it('accepts only this service attachment URLs for images', () => {
    expect(isAttachmentUrl(FILE)).toBe(true);
    expect(isAttachmentUrl(`${FILE}?inline=true`)).toBe(true);
    expect(isAttachmentUrl(`${FILE}?version=3&inline=true`)).toBe(true);
    expect(isAttachmentUrl(`${FILE}?version=03`)).toBe(false);
    expect(isAttachmentUrl('https://tracker.example/pixel.gif')).toBe(false);
    expect(isAttachmentUrl('/api/files/NOT-A-UUID/content')).toBe(false);
  });

  it('accepts http(s), mailto and relative links; rejects other schemes and //host', () => {
    expect(isAllowedHref('https://example.test/a')).toBe(true);
    expect(isAllowedHref('mailto:a@example.test')).toBe(true);
    expect(isAllowedHref('/w/x/wiki/y')).toBe(true);
    expect(isAllowedHref('javascript:alert(1)')).toBe(false);
    expect(isAllowedHref('data:text/html,x')).toBe(false);
    expect(isAllowedHref('//evil.example')).toBe(false);
  });

  it('drops outside images and unsafe link marks from a pasted slice, keeping the text', () => {
    const editor = new Editor({ extensions: schemaExtensions() });
    editors.push(editor);
    const { schema } = editor;
    const link = schema.marks.link;
    const image = schema.nodes.image;
    const paragraph = schema.nodes.paragraph;
    if (link === undefined || image === undefined || paragraph === undefined)
      throw new Error('schema');
    const doc = schema.nodes.doc?.create(null, [
      paragraph.create(null, [
        schema.text('bad', [link.create({ href: 'javascript:alert(1)' })]),
        schema.text(' good', [link.create({ href: 'https://ok.example' })]),
      ]),
      image.create({ src: 'https://tracker.example/x.gif' }),
      image.create({ src: FILE }),
    ]);
    if (doc === undefined) throw new Error('doc');
    const cleaned = cleanSlice(new Slice(doc.content, 0, 0));
    const json: unknown = cleaned.content.toJSON();
    const text = JSON.stringify(json);
    expect(text).toContain('"text":"bad"');
    expect(text).not.toContain('javascript:');
    expect(text).toContain('https://ok.example');
    expect(text).not.toContain('tracker.example');
    expect(text).toContain(FILE);
  });
});

describe('link input', () => {
  it('normalizes host-like input to https and rejects scripts', () => {
    expect(normalizeHref('example.com/a')).toBe('https://example.com/a');
    expect(normalizeHref(' https://x.test ')).toBe('https://x.test');
    expect(normalizeHref('mailto:a@b.test')).toBe('mailto:a@b.test');
    expect(normalizeHref('javascript:alert(1)')).toBeUndefined();
    expect(normalizeHref('')).toBeUndefined();
  });
});

describe('slash menu', () => {
  const item = (id: string, label: string, keywords: string[]): SlashItem => ({
    id,
    label,
    hint: '',
    keywords,
    run: () => undefined,
  });
  const items = [
    item('paragraph', '텍스트', ['text']),
    item('task', '체크리스트', ['todo', '할 일', '체크']),
    item('table', '표', ['table']),
  ];

  it('filters by Korean or English keywords, prefix matches first', () => {
    expect(filterSlashItems(items, '').map((i) => i.id)).toEqual(['paragraph', 'task', 'table']);
    expect(filterSlashItems(items, '체크').map((i) => i.id)).toEqual(['task']);
    expect(filterSlashItems(items, 'TO').map((i) => i.id)).toEqual(['task']);
    expect(filterSlashItems(items, 'xyz')).toEqual([]);
  });

  it('leaves keys pressed during IME composition to the input method', () => {
    expect(isComposingKey(new KeyboardEvent('keydown', { key: 'Enter', isComposing: true }))).toBe(
      true,
    );
    expect(isComposingKey(new KeyboardEvent('keydown', { key: 'Process' }))).toBe(true);
    // Safari: 조합을 끝내는 Enter에서 isComposing이 거짓이어도 편집기의 조합 상태로 막는다.
    expect(isComposingKey(new KeyboardEvent('keydown', { key: 'Enter' }), true)).toBe(true);
    expect(isComposingKey(new KeyboardEvent('keydown', { key: 'Enter' }))).toBe(false);
  });
});
