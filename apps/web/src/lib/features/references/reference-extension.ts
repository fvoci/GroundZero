// @ 참조 노드와 입력 후보 (D-18).
// 노드는 종류와 id만 저장한다(표시 이름은 볼 때 권한을 확인해 만든다). 후보 목록은 키보드로 고르고
// 편집기 쪽에 aria-activedescendant를 알려 화면 낭독기에서도 따라갈 수 있게 한다.
import type { Editor } from '@tiptap/core';
import { Extension, Node, createInlineMarkdownSpec, mergeAttributes } from '@tiptap/core';
import type { SuggestionKeyDownProps, SuggestionProps } from '@tiptap/suggestion';
import Suggestion from '@tiptap/suggestion';
import { computePosition, flip, offset, shift } from '@floating-ui/dom';

import type { ReferenceCandidateView, ReferenceKind, ResolvedReferenceView } from '$lib/api/client';
import { api } from '$lib/api/client';
import { m } from '$lib/paraglide/messages.js';

import { referenceLabel, resolveReference } from './resolver';

/** 종류 이름. 언어가 바뀔 수 있어 부를 때 만든다. */
function kindLabel(kind: ReferenceKind): string {
  switch (kind) {
    case 'item':
      return m.references_kind_item();
    case 'document':
      return m.references_kind_document();
    case 'task':
      return m.references_kind_task();
    case 'user':
      return m.references_kind_user();
  }
}

function asKind(value: unknown): ReferenceKind | undefined {
  return value === 'item' || value === 'document' || value === 'task' || value === 'user'
    ? value
    : undefined;
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

  // Markdown 모드에서도 참조가 사라지지 않게 `[reference kind="item" id="…"]` 형식으로 쓰고 읽는다(D-27).
  ...createInlineMarkdownSpec({
    nodeName: 'reference',
    selfClosing: true,
    allowedAttributes: ['kind', 'id'],
  }),

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
        dom.textContent = m.references_hidden();
        return { dom };
      }
      void resolveReference(kind, rawId).then((target) => {
        // 볼 수 없는 대상의 문구는 화면 언어로(내보내기 파일은 resolver의 고정 문구를 쓴다).
        dom.textContent =
          target.visible && target.title !== null ? referenceLabel(target) : m.references_hidden();
        dom.title = target.visible ? kindLabel(kind) : '';
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

/** 워크스페이스 id → 표시 이름(같은 이름이면 보조 정보 포함)과 배지 색. 내가 속하지 않았으면 없다. */
export type DescribeWorkspace = (id: string) => { label: string; color: string } | undefined;

function createPopup(describeWorkspace: DescribeWorkspace): {
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
        props.query === '' ? m.references_suggest_prompt() : m.references_suggest_empty();
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
      kind.textContent = kindLabel(item.kind);
      const reference = document.createElement('span');
      reference.className = 'shrink-0 font-mono text-xs text-muted-foreground';
      reference.textContent = item.reference ?? '';
      const title = document.createElement('span');
      title.textContent = item.title;
      const where = document.createElement('span');
      where.className = 'ml-auto shrink-0 text-xs text-muted-foreground';
      const ws = item.workspace_id === null ? undefined : describeWorkspace(item.workspace_id);
      if (ws === undefined) {
        where.textContent = item.workspace_name ?? '';
      } else {
        const dot = document.createElement('span');
        dot.setAttribute('aria-hidden', 'true');
        dot.className = `mr-1 inline-block size-2 rounded-full ${ws.color}`;
        where.append(dot, ws.label);
      }
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
      el.setAttribute('aria-label', m.references_suggest_label());
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
  describeWorkspace: DescribeWorkspace;
}

export const ReferenceSuggestion = Extension.create<ReferenceSuggestionOptions>({
  name: 'referenceSuggestion',

  addOptions() {
    return { workspaceId: null, describeWorkspace: () => undefined };
  },

  addProseMirrorPlugins() {
    const { workspaceId, describeWorkspace } = this.options;
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
        render: () => createPopup(describeWorkspace),
      }),
    ];
  },
});
