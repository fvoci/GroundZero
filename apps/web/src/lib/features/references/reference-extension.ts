// @ 참조 노드와 입력 후보 (D-18).
// 노드는 종류와 id만 저장한다(표시 이름은 볼 때 권한을 확인해 만든다). 후보 목록은 키보드로 고르고
// 편집기 쪽에 aria-activedescendant를 알려 화면 낭독기에서도 따라갈 수 있게 한다.
import type { Editor } from '@tiptap/core';
import { Extension, Node, mergeAttributes } from '@tiptap/core';
import type { SuggestionKeyDownProps, SuggestionProps } from '@tiptap/suggestion';
import Suggestion from '@tiptap/suggestion';
import { computePosition, flip, offset, shift } from '@floating-ui/dom';

import type { ReferenceCandidateView, ReferenceKind, ResolvedReferenceView } from '$lib/api/client';
import { api } from '$lib/api/client';

import { referenceLabel, resolveReference } from './resolver';

const KIND_LABEL: Record<ReferenceKind, string> = { item: '항목', document: '문서', user: '사람' };

function asKind(value: unknown): ReferenceKind | undefined {
  return value === 'item' || value === 'document' || value === 'user' ? value : undefined;
}

export interface ReferenceOptions {
  /** 대상으로 가는 앱 경로. 갈 수 없으면 null. */
  hrefFor: (target: ResolvedReferenceView) => string | null;
  navigate: (path: string) => void;
}

export const Reference = Node.create<ReferenceOptions>({
  name: 'reference',
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,

  addOptions() {
    return { hrefFor: () => null, navigate: () => undefined };
  },

  addAttributes() {
    return {
      kind: {
        default: null,
        parseHTML: (el: HTMLElement) => el.getAttribute('data-kind'),
        renderHTML: (attrs: Record<string, unknown>) => ({ 'data-kind': attrs.kind }),
      },
      id: {
        default: null,
        parseHTML: (el: HTMLElement) => el.getAttribute('data-id'),
        renderHTML: (attrs: Record<string, unknown>) => ({ 'data-id': attrs.id }),
      },
    };
  },

  parseHTML() {
    return [{ tag: 'span[data-reference]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['span', mergeAttributes({ 'data-reference': '' }, HTMLAttributes)];
  },

  renderText({ node }) {
    const kind: unknown = node.attrs.kind;
    const id: unknown = node.attrs.id;
    return `@[${String(kind)}:${String(id)}]`;
  },

  addNodeView() {
    const { hrefFor, navigate } = this.options;
    return ({ node, editor }) => {
      const dom = document.createElement('a');
      dom.setAttribute('data-reference', '');
      dom.className =
        'rounded bg-accent px-1 text-accent-foreground no-underline focus-visible:ring-2 focus-visible:ring-ring';
      dom.textContent = '…';
      const kind = asKind(node.attrs.kind);
      const rawId: unknown = node.attrs.id;
      if (kind === undefined || typeof rawId !== 'string') {
        dom.textContent = '볼 수 없는 대상';
        return { dom };
      }
      void resolveReference(kind, rawId).then((target) => {
        dom.textContent = referenceLabel(target);
        dom.title = target.visible ? KIND_LABEL[kind] : '';
        const href = hrefFor(target);
        if (href === null) dom.removeAttribute('href');
        else dom.href = href;
      });
      dom.addEventListener('click', (event: MouseEvent) => {
        const href = dom.getAttribute('href');
        event.preventDefault();
        // 편집 중에는 클릭이 선택이다. Ctrl·Cmd 클릭이나 읽기 화면에서만 이동한다.
        if (href === null || (editor.isEditable && !event.metaKey && !event.ctrlKey)) return;
        navigate(href);
      });
      return { dom };
    };
  },
});

// --- 입력 후보 -----------------------------------------------------------------

interface PopupState {
  el: HTMLDivElement;
  items: ReferenceCandidateView[];
  selected: number;
  props: SuggestionProps<ReferenceCandidateView, ReferenceCandidateView>;
}

function optionId(listId: string, index: number): string {
  return `${listId}-${String(index)}`;
}

function createPopup(): {
  onStart: (props: SuggestionProps<ReferenceCandidateView, ReferenceCandidateView>) => void;
  onUpdate: (props: SuggestionProps<ReferenceCandidateView, ReferenceCandidateView>) => void;
  onKeyDown: (props: SuggestionKeyDownProps) => boolean;
  onExit: () => void;
} {
  const listId = `reference-options-${crypto.randomUUID()}`;
  let state: PopupState | undefined;

  function editorDom(editor: Editor): HTMLElement {
    return editor.view.dom;
  }

  function draw(): void {
    if (state === undefined) return;
    const { el, items, selected, props } = state;
    el.replaceChildren();
    if (items.length === 0) {
      const empty = document.createElement('div');
      empty.className = 'px-2 py-1.5 text-sm text-muted-foreground';
      empty.textContent =
        props.query === '' ? '이름·제목·번호를 입력하세요' : '찾는 대상이 없습니다';
      el.append(empty);
    }
    items.forEach((item, index) => {
      const option = document.createElement('div');
      option.id = optionId(listId, index);
      option.setAttribute('role', 'option');
      option.setAttribute('aria-selected', String(index === selected));
      option.className =
        'flex cursor-pointer items-baseline gap-2 rounded-sm px-2 py-1.5 text-sm ' +
        (index === selected ? 'bg-accent text-accent-foreground' : '');
      const kind = document.createElement('span');
      kind.className = 'shrink-0 text-xs text-muted-foreground';
      kind.textContent = KIND_LABEL[item.kind];
      const reference = document.createElement('span');
      reference.className = 'shrink-0 font-mono text-xs text-muted-foreground';
      reference.textContent = item.reference ?? '';
      const title = document.createElement('span');
      title.textContent = item.title;
      const where = document.createElement('span');
      where.className = 'ml-auto shrink-0 text-xs text-muted-foreground';
      where.textContent = item.workspace_name ?? '';
      option.append(kind, reference, title, where);
      option.addEventListener('mousedown', (event: MouseEvent) => {
        event.preventDefault();
        props.command(item);
      });
      el.append(option);
    });
    const dom = editorDom(props.editor);
    if (items.length > 0) dom.setAttribute('aria-activedescendant', optionId(listId, selected));
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

  function update(props: SuggestionProps<ReferenceCandidateView, ReferenceCandidateView>): void {
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
    onStart(props) {
      const el = document.createElement('div');
      el.id = listId;
      el.setAttribute('role', 'listbox');
      el.setAttribute('aria-label', '참조 후보');
      el.className =
        'fixed z-50 flex max-h-72 min-w-64 max-w-[min(32rem,calc(100vw-2rem))] flex-col overflow-y-auto rounded-md border bg-popover p-1 text-popover-foreground shadow-md';
      document.body.append(el);
      const dom = editorDom(props.editor);
      dom.setAttribute('aria-controls', listId);
      dom.setAttribute('aria-expanded', 'true');
      state = { el, items: [], selected: 0, props };
      update(props);
    },
    onUpdate: update,
    onKeyDown({ event }) {
      if (state === undefined) return false;
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

export interface ReferenceSuggestionOptions {
  /** 후보를 찾을 워크스페이스. 개인 노트는 null(내가 속한 모든 워크스페이스). */
  workspaceId: string | null;
}

export const ReferenceSuggestion = Extension.create<ReferenceSuggestionOptions>({
  name: 'referenceSuggestion',

  addOptions() {
    return { workspaceId: null };
  },

  addProseMirrorPlugins() {
    const { workspaceId } = this.options;
    return [
      Suggestion<ReferenceCandidateView, ReferenceCandidateView>({
        editor: this.editor,
        char: '@',
        items: async ({ query }) => {
          try {
            return await api.referenceCandidates(query, workspaceId);
          } catch {
            return [];
          }
        },
        command: ({ editor, range, props }) => {
          editor
            .chain()
            .focus()
            .insertContentAt(range, [
              { type: 'reference', attrs: { kind: props.kind, id: props.id } },
              { type: 'text', text: ' ' },
            ])
            .run();
        },
        render: createPopup,
      }),
    ];
  },
});
