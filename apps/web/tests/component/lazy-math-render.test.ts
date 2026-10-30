import { Editor, type JSONContent } from '@tiptap/core';
import { fireEvent, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { schemaExtensions } from '$lib/features/documents/editor-schema';
import { loadKatex, type MathRenderer } from '$lib/features/documents/katex-loader';
import { setLocale } from '$lib/paraglide/runtime.js';

vi.mock('$lib/features/documents/katex-loader', () => ({ loadKatex: vi.fn() }));

const editors: Editor[] = [];
const source = 'x^2 + y^2';
const inline = { type: 'paragraph', content: [{ type: 'inlineMath', attrs: { latex: source } }] };
const formula = {
  type: 'doc',
  content: [inline, { type: 'blockMath', attrs: { latex: '\\sum_{i=1}^n i' } }],
};

function editor(content: JSONContent, clicked: () => void = () => undefined): Editor {
  const element = document.createElement('div');
  document.body.append(element);
  const instance = new Editor({
    element,
    content,
    extensions: schemaExtensions({ onMathClick: clicked }),
  });
  editors.push(instance);
  return instance;
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.mocked(loadKatex).mockReset();
});
afterEach(() => {
  for (const instance of editors.splice(0)) instance.destroy();
  document.body.replaceChildren();
});

it('does not load the renderer for a document without formulas', async () => {
  editor({
    type: 'doc',
    content: [{ type: 'paragraph', content: [{ type: 'text', text: '본문' }] }],
  });
  await Promise.resolve();
  expect(loadKatex).not.toHaveBeenCalled();
});

it('does not load math for detached serialization-only editors', async () => {
  const instance = new Editor({ content: formula, extensions: schemaExtensions() });
  editors.push(instance);
  expect(instance.getHTML()).toContain('data-latex');
  await Promise.resolve();
  expect(loadKatex).not.toHaveBeenCalled();
});

it('keeps TeX while loading, renders both node kinds, and preserves serialized formulas', async () => {
  let finish: (renderer: MathRenderer) => void = () => undefined;
  const pending = new Promise<MathRenderer>((resolve) => {
    finish = resolve;
  });
  vi.mocked(loadKatex).mockReturnValue(pending);
  const instance = editor(formula);
  await waitFor(() => {
    expect(loadKatex).toHaveBeenCalled();
  });
  expect(document.querySelector('.katex')).toBeNull();
  expect(document.querySelector('[data-type="inline-math"]')).toHaveTextContent(source);
  const changed = vi.fn();
  const before = instance.getJSON();
  const htmlBefore = instance.getHTML();
  instance.on('update', changed);
  const actual = (await vi.importActual<typeof import('katex')>('katex')).default;
  finish(actual);
  await waitFor(() => {
    expect(document.querySelectorAll('.katex')).toHaveLength(2);
  });
  expect(instance.getJSON()).toEqual(before);
  expect(instance.getHTML()).toEqual(htmlBefore);
  expect(changed).not.toHaveBeenCalled();
});

it('keeps source on load failure and retries without opening the math edit command', async () => {
  vi.mocked(loadKatex).mockRejectedValue(new Error('renderer unavailable'));
  const clicked = vi.fn();
  const instance = editor({ type: 'doc', content: [inline] }, clicked);
  const before = instance.getJSON();
  await waitFor(() => {
    expect(document.querySelector('button')).toBeVisible();
  });
  expect(document.querySelector('.katex')).toBeNull();
  expect(document.body).toHaveTextContent(source);
  const retry = document.querySelector('button');
  if (retry === null) throw new Error('retry');
  const actual = (await vi.importActual<typeof import('katex')>('katex')).default;
  vi.mocked(loadKatex).mockResolvedValue(actual);
  await fireEvent.click(retry);
  await waitFor(() => {
    expect(document.querySelector('.katex')).not.toBeNull();
  });
  expect(instance.getJSON()).toEqual(before);
  expect(clicked).not.toHaveBeenCalled();
});
