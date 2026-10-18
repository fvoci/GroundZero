import { Editor } from '@tiptap/core';
import { Fragment, Slice } from '@tiptap/pm/model';
import { afterEach, describe, expect, it } from 'vitest';

import { schemaExtensions } from '$lib/features/documents/editor-schema';
import { normalizeHref } from '$lib/features/documents/link-href';
import type { ContentAdjustments } from '$lib/features/documents/paste-guard';
import {
  cleanSlice,
  contentIssues,
  PasteGuard,
  isAllowedHref,
  isAttachmentUrl,
  noAdjustments,
  SafeImageInput,
  cleanPastedHtml,
} from '$lib/features/documents/paste-guard';
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
    expect(isAllowedHref(`https://a.example/${String.fromCharCode(7)}x`)).toBe(false);
    expect(isAllowedHref('/api/files/not-canonical/content')).toBe(false);
    expect(isAllowedHref(FILE)).toBe(true);
  });

  it('turns outside images into links and unlinks unsafe links, reporting what changed', () => {
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
      image.create({ src: 'https://tracker.example/x.gif', alt: '그림' }),
      image.create({ src: 'data:image/png;base64,AAAA' }),
      image.create({ src: FILE }),
    ]);
    if (doc === undefined) throw new Error('doc');
    const stats = noAdjustments();
    const cleaned = cleanSlice(new Slice(doc.content, 0, 0), stats);
    const text = JSON.stringify(cleaned.content.toJSON());
    expect(stats).toEqual({ imagesLinked: 1, imagesRemoved: 1, linksUnlinked: 1 });
    expect(text).toContain('"text":"bad"');
    expect(text).not.toContain('javascript:');
    expect(text).toContain('https://ok.example');
    // 바깥 이미지는 지우지 않고 같은 주소의 링크 글로 남는다(설명이 글이 된다).
    expect(text).toContain('"text":"그림"');
    expect(text).toContain('"href":"https://tracker.example/x.gif"');
    expect(text).not.toContain('"type":"image","attrs":{"src":"https://tracker.example');
    expect(text).not.toContain('data:image');
    expect(text).toContain(FILE);
  });

  it('cleans pasted HTML before the editor reads it, counting each change once', () => {
    const stats = noAdjustments();
    const html = cleanPastedHtml(
      '<p>글<img src="https://outside.example/a.png" alt="풍경"><img src="data:image/png;base64,AA"><a href="javascript:alert(1)">나쁜</a><a href="https://ok.example">좋은</a></p><img src="' +
        FILE +
        '">',
      stats,
    );
    expect(stats).toEqual({ imagesLinked: 1, imagesRemoved: 1, linksUnlinked: 1 });
    expect(html).toContain('<a href="https://outside.example/a.png">풍경</a>');
    expect(html).not.toContain('data:image');
    expect(html).not.toContain('javascript:');
    expect(html).toContain('나쁜');
    expect(html).toContain('href="https://ok.example"');
    expect(html).toContain(FILE);
  });

  it('finds content the server would reject before saving', () => {
    const issues = contentIssues({
      type: 'doc',
      content: [
        { type: 'image', attrs: { src: 'https://outside.example/a.png' } },
        { type: 'image', attrs: { src: FILE } },
        {
          type: 'paragraph',
          content: [
            {
              type: 'text',
              text: 'x',
              marks: [{ type: 'link', attrs: { href: '//evil.example' } }],
            },
            { type: 'text', text: 'y', marks: [{ type: 'link', attrs: { href: '/w/a/wiki/b' } }] },
          ],
        },
      ],
    });
    expect(issues).toEqual({
      images: ['https://outside.example/a.png'],
      links: ['//evil.example'],
    });
    expect(contentIssues({ type: 'doc', content: [{ type: 'paragraph' }] })).toEqual({
      images: [],
      links: [],
    });
  });

  it('does not load outside images that came in through Markdown or a draft', () => {
    const editor = new Editor({
      extensions: schemaExtensions(),
      content: {
        type: 'doc',
        content: [
          { type: 'image', attrs: { src: 'https://outside.example/md.png' } },
          { type: 'image', attrs: { src: FILE } },
        ],
      },
    });
    editors.push(editor);
    const dom = editor.view.dom;
    expect(dom.querySelector('img[src^="https://outside"]')).toBeNull();
    expect(dom.querySelector('[data-blocked-image]')?.textContent).toContain(
      'https://outside.example/md.png',
    );
    expect(dom.querySelector(`img[src="${FILE}"]`)).not.toBeNull();
    // 내용은 그대로 남는다(저장 전 검사가 알린다).
    expect(JSON.stringify(editor.getJSON())).toContain('https://outside.example/md.png');
  });

  it('typing ![alt](url) inserts attachments as images and outside images as links', () => {
    let linked = 0;
    const editor = new Editor({
      extensions: [
        ...schemaExtensions(),
        SafeImageInput.configure({
          onLinked: () => {
            linked += 1;
          },
        }),
      ],
      content: { type: 'doc', content: [{ type: 'paragraph' }] },
    });
    editors.push(editor);
    const type = (text: string): void => {
      for (const ch of text) {
        const { from, to } = editor.state.selection;
        const handled = editor.view.someProp('handleTextInput', (f) =>
          f(editor.view, from, to, ch, () => editor.state.tr.insertText(ch, from, to)),
        );
        if (handled !== true) editor.view.dispatch(editor.state.tr.insertText(ch, from, to));
      }
    };
    type('![사진](https://outside.example/p.png)');
    const json = JSON.stringify(editor.getJSON());
    expect(linked).toBe(1);
    expect(json).not.toContain('"type":"image"');
    expect(json).toContain('"href":"https://outside.example/p.png"');
    expect(json).toContain('"text":"사진"');
    editor.commands.setContent({ type: 'doc', content: [{ type: 'paragraph' }] });
    type(`![첨부](${FILE})`);
    expect(JSON.stringify(editor.getJSON())).toContain(`"src":"${FILE}"`);
    expect(linked).toBe(1);
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

describe('review fixes', () => {
  // jsdom에는 ClipboardEvent가 없다. ProseMirror의 pasteHTML·pasteText는 이벤트를 만들기만 한다.
  if (!('ClipboardEvent' in globalThis))
    Object.defineProperty(globalThis, 'ClipboardEvent', {
      value: class extends Event {},
      configurable: true,
    });

  const typeInto = (editor: Editor, text: string): void => {
    for (const ch of text) {
      const { from, to } = editor.state.selection;
      const handled = editor.view.someProp('handleTextInput', (f) =>
        f(editor.view, from, to, ch, () => editor.state.tr.insertText(ch, from, to)),
      );
      if (handled !== true) editor.view.dispatch(editor.state.tr.insertText(ch, from, to));
    }
  };

  it('a literal ![…](…) earlier in the paragraph is not swallowed into the next one', () => {
    const editor = new Editor({
      extensions: [...schemaExtensions(), SafeImageInput],
      content: { type: 'doc', content: [{ type: 'paragraph' }] },
    });
    editors.push(editor);
    typeInto(editor, 'see ![a](javascript:x) and ![b](https://y.example/p.png)');
    const paragraph = editor.getJSON().content[0];
    const json = JSON.stringify(paragraph);
    // 앞의 글자 그대로인 ![a](javascript:x)는 남고, 뒤의 것만 설명 b의 링크가 된다.
    expect(json).toContain('"text":"see ![a](javascript:x) and "');
    expect(json).toContain('}],"text":"b"}');
    expect(JSON.stringify(paragraph)).toContain('"href":"https://y.example/p.png"');
  });

  it('leaves pasted HTML untouched when nothing needs cleaning (styles and bare cells survive)', () => {
    const office =
      '<style>.xl65{font-weight:700}</style><table><tr><td class="xl65">굵게</td></tr></table>';
    expect(cleanPastedHtml(office, noAdjustments())).toBe(office);
    const cells = '<td>a</td><td>b</td>';
    expect(cleanPastedHtml(cells, noAdjustments())).toBe(cells);
    const stats = noAdjustments();
    const fixed = cleanPastedHtml(
      '<style>.b{font-weight:700}</style><td class="b"><img src="https://o.example/x.png"></td>',
      stats,
    );
    expect(stats.imagesLinked).toBe(1);
    expect(fixed).toContain('<style>');
    expect(fixed).toContain('<td class="b"><a href="https://o.example/x.png">');
  });

  it('counts a link spread over several text pieces once', () => {
    const editor = new Editor({ extensions: schemaExtensions() });
    editors.push(editor);
    const { schema } = editor;
    const link = schema.marks.link?.create({ href: '//evil.example' });
    const bold = schema.marks.bold?.create();
    if (link === undefined || bold === undefined) throw new Error('schema');
    const paragraph = schema.nodes.paragraph?.create(null, [
      schema.text('a', [bold, link]),
      schema.text(' b', [link]),
      schema.text(' 사이 '),
      schema.text('c', [link]),
    ]);
    if (paragraph === undefined) throw new Error('paragraph');
    const stats = noAdjustments();
    cleanSlice(new Slice(Fragment.from(paragraph), 0, 0), stats);
    expect(stats.linksUnlinked).toBe(2);
  });

  it('a paste through the editor reports each change once', () => {
    const reported: ContentAdjustments[] = [];
    const editor = new Editor({
      extensions: [
        ...schemaExtensions(),
        PasteGuard.configure({ onAdjusted: (stats) => reported.push({ ...stats }) }),
      ],
      content: { type: 'doc', content: [{ type: 'paragraph' }] },
    });
    editors.push(editor);
    editor.view.pasteHTML(
      '<p>글<img src="https://o.example/a.png" alt="풍경"><img src="data:image/png;base64,AA"><a href="javascript:alert(1)">나쁜</a></p>',
    );
    expect(reported).toEqual([{ imagesLinked: 1, imagesRemoved: 1, linksUnlinked: 1 }]);
    const json = JSON.stringify(editor.getJSON());
    expect(json).toContain('"href":"https://o.example/a.png"');
    expect(json).not.toContain('data:image');
    expect(json).not.toContain('javascript:');
  });

  it('does not autolink or paste-link addresses the server would reject', () => {
    const editor = new Editor({
      extensions: schemaExtensions(),
      content: { type: 'doc', content: [{ type: 'paragraph' }] },
    });
    editors.push(editor);
    typeInto(editor, 'ftp://files.example/a https://ok.example/b ');
    const typed = JSON.stringify(editor.getJSON());
    expect(typed).not.toContain('"href":"ftp:');
    expect(typed).toContain('"href":"https://ok.example/b"');
    editor.commands.setContent({ type: 'doc', content: [{ type: 'paragraph' }] });
    editor.view.pasteText('ftp://files.example/c');
    expect(JSON.stringify(editor.getJSON())).not.toContain('"href"');
  });
});
