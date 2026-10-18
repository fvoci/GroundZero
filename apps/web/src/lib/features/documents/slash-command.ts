// / 명령 메뉴. 블록을 찾아 넣는다(@tiptap/suggestion, @ 참조 입력과 같은 방식). 목록에는 이 편집기에서 실제로
// 할 수 있는 항목만 둔다(첨부·참조는 그 기능이 있을 때만). 한글 조합 중의 Enter·방향키는 조합을 끝내는 데 쓰이므로
// 메뉴가 가로채지 않는다.
import type { Editor, Range } from '@tiptap/core';
import { Extension } from '@tiptap/core';
import { computePosition, flip, offset, shift } from '@floating-ui/dom';
import type { SuggestionKeyDownProps, SuggestionProps } from '@tiptap/suggestion';
import { Suggestion } from '@tiptap/suggestion';
import { PluginKey } from '@tiptap/pm/state';

import { m } from '$lib/paraglide/messages.js';

export interface SlashItem {
  id: string;
  label: string;
  hint: string;
  keywords: string[];
  run: (editor: Editor, range: Range) => void;
}

/** 검색어로 거른다. 비면 전부. 이름·검색어 앞부분이 맞는 것을 먼저 둔다. */
export function filterSlashItems(items: readonly SlashItem[], query: string): SlashItem[] {
  const q = query.trim().toLowerCase();
  if (q === '') return [...items];
  const score = (item: SlashItem): number => {
    const words = [item.label.toLowerCase(), ...item.keywords.map((k) => k.toLowerCase())];
    if (words.some((w) => w.startsWith(q))) return 2;
    if (words.some((w) => w.includes(q))) return 1;
    return 0;
  };
  return items
    .map((item, index) => ({ item, index, s: score(item) }))
    .filter((x) => x.s > 0)
    .sort((a, b) => b.s - a.s || a.index - b.index)
    .map((x) => x.item);
}

/**
 * 한글·일본어 등 입력기 조합 중의 키(조합을 끝내는 Enter 등)는 메뉴가 쓰지 않는다. Safari는 조합을 끝내는
 * Enter에서 isComposing이 거짓일 수 있어 ProseMirror가 아는 조합 상태(view.composing)도 본다.
 */
export function isComposingKey(event: KeyboardEvent, composing = false): boolean {
  return event.isComposing || event.key === 'Process' || composing;
}

let popupSeq = 0;

function createPopup(): {
  onStart: (props: SuggestionProps<SlashItem, SlashItem>) => void;
  onUpdate: (props: SuggestionProps<SlashItem, SlashItem>) => void;
  onKeyDown: (props: SuggestionKeyDownProps) => boolean;
  onExit: () => void;
} {
  const listId = `slash-menu-${String(++popupSeq)}`;
  let state:
    | {
        el: HTMLDivElement;
        items: SlashItem[];
        selected: number;
        props: SuggestionProps<SlashItem, SlashItem>;
      }
    | undefined;

  const optionId = (i: number): string => `${listId}-${String(i)}`;
  const editorDom = (editor: Editor): HTMLElement => editor.view.dom;

  function draw(): void {
    if (state === undefined) return;
    const { el, items, selected, props } = state;
    el.replaceChildren();
    if (items.length === 0) {
      const empty = document.createElement('p');
      empty.className = 'px-2 py-1.5 text-sm text-muted-foreground';
      empty.textContent = m.documents_slash_empty();
      el.append(empty);
    }
    items.forEach((item, i) => {
      const option = document.createElement('div');
      option.id = optionId(i);
      option.setAttribute('role', 'option');
      option.setAttribute('aria-selected', String(i === selected));
      option.className = `flex cursor-pointer flex-col rounded-sm px-2 py-1.5 text-sm ${i === selected ? 'bg-accent text-accent-foreground' : ''}`;
      const label = document.createElement('span');
      label.className = 'font-medium';
      label.textContent = item.label;
      const hint = document.createElement('span');
      hint.className = 'text-xs text-muted-foreground';
      hint.textContent = item.hint;
      option.append(label, hint);
      option.addEventListener('mousedown', (event: MouseEvent) => {
        event.preventDefault();
        props.command(item);
      });
      el.append(option);
      if (i === selected) option.scrollIntoView({ block: 'nearest' });
    });
    const dom = editorDom(props.editor);
    if (items.length > 0) dom.setAttribute('aria-activedescendant', optionId(selected));
    else dom.removeAttribute('aria-activedescendant');
  }

  function place(): void {
    if (state === undefined) return;
    const rect = state.props.clientRect?.();
    if (rect === null || rect === undefined) return;
    const el = state.el;
    void computePosition({ getBoundingClientRect: () => rect }, el, {
      placement: 'bottom-start',
      strategy: 'fixed',
      middleware: [offset(4), flip(), shift({ padding: 8 })],
    }).then(({ x, y }) => {
      el.style.left = `${String(x)}px`;
      el.style.top = `${String(y)}px`;
    });
  }

  function update(props: SuggestionProps<SlashItem, SlashItem>): void {
    if (state === undefined) return;
    state.props = props;
    state.items = props.items;
    state.selected = Math.min(state.selected, Math.max(props.items.length - 1, 0));
    draw();
    place();
  }

  function close(): void {
    if (state === undefined) return;
    const dom = editorDom(state.props.editor);
    dom.removeAttribute('aria-activedescendant');
    dom.removeAttribute('aria-controls');
    dom.setAttribute('aria-expanded', 'false');
    state.el.remove();
    state = undefined;
  }

  return {
    onStart(props: SuggestionProps<SlashItem, SlashItem>) {
      const el = document.createElement('div');
      el.id = listId;
      el.setAttribute('role', 'listbox');
      el.setAttribute('aria-label', m.documents_slash_label());
      el.className =
        'fixed z-50 flex max-h-80 w-72 max-w-[calc(100vw-2rem)] flex-col overflow-y-auto rounded-md border bg-popover p-1 text-popover-foreground shadow-md';
      document.body.append(el);
      const dom = editorDom(props.editor);
      dom.setAttribute('aria-controls', listId);
      dom.setAttribute('aria-expanded', 'true');
      state = { el, items: [], selected: 0, props };
      update(props);
    },
    onUpdate: update,
    onKeyDown({ event, view }: SuggestionKeyDownProps) {
      if (state === undefined || isComposingKey(event, view.composing)) return false;
      const count = state.items.length;
      if (event.key === 'ArrowDown' && count > 0) {
        state.selected = (state.selected + 1) % count;
        draw();
        return true;
      }
      if (event.key === 'ArrowUp' && count > 0) {
        state.selected = (state.selected - 1 + count) % count;
        draw();
        return true;
      }
      if (event.key === 'Enter' || event.key === 'Tab') {
        const item = state.items[state.selected];
        if (item === undefined) return false;
        state.props.command(item);
        return true;
      }
      if (event.key === 'Escape') {
        close();
        return true;
      }
      return false;
    },
    onExit: close,
  };
}

export const slashPluginKey = new PluginKey('slashCommand');

export interface SlashCommandOptions {
  /** 지금 넣을 수 있는 항목. 열 때마다 부른다. */
  items: () => SlashItem[];
}

export const SlashCommand = Extension.create<SlashCommandOptions>({
  name: 'slashCommand',

  addOptions() {
    return { items: () => [] };
  },

  addProseMirrorPlugins() {
    const { items } = this.options;
    return [
      Suggestion<SlashItem, SlashItem>({
        editor: this.editor,
        pluginKey: slashPluginKey,
        char: '/',
        // 코드 블록 안의 /는 글자다.
        allow: ({ editor, state }) =>
          editor.isEditable && state.selection.$from.parent.type.spec.code !== true,
        items: ({ query }) => filterSlashItems(items(), query),
        command: ({ editor, range, props }) => {
          props.run(editor, range);
        },
        render: createPopup,
      }),
    ];
  },
});
