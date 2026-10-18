// 블록 유형(문단·제목·목록·체크리스트·인용·코드). / 메뉴와 선택 도구 막대의 "유형 바꾸기"가 같은 명령을 쓴다.
// 모두 기존 스키마의 노드이고 Tiptap 명령 한 번(한 트랜잭션)이다.
import type { Editor } from '@tiptap/core';

import { m } from '$lib/paraglide/messages.js';

export const BLOCK_KINDS = [
  'paragraph',
  'h1',
  'h2',
  'h3',
  'bullet',
  'ordered',
  'task',
  'quote',
  'code',
] as const;

export type BlockKind = (typeof BLOCK_KINDS)[number];

/** 지금 커서가 있는 블록의 유형. 안쪽 것을 먼저 본다(목록 안 코드 블록이면 코드). */
export function activeBlockKind(editor: Editor): BlockKind {
  if (editor.isActive('codeBlock')) return 'code';
  if (editor.isActive('heading', { level: 1 })) return 'h1';
  if (editor.isActive('heading', { level: 2 })) return 'h2';
  if (editor.isActive('heading', { level: 3 })) return 'h3';
  if (editor.isActive('taskList')) return 'task';
  if (editor.isActive('orderedList')) return 'ordered';
  if (editor.isActive('bulletList')) return 'bullet';
  if (editor.isActive('blockquote')) return 'quote';
  return 'paragraph';
}

/** 선택한 블록을 이 유형으로 바꾼다. 문단·제목은 목록·인용 밖으로 꺼낸다. */
export function setBlockKind(editor: Editor, kind: BlockKind): boolean {
  const chain = editor.chain().focus();
  switch (kind) {
    case 'paragraph':
      return chain.clearNodes().setParagraph().run();
    case 'h1':
    case 'h2':
    case 'h3':
      return chain
        .clearNodes()
        .setHeading({ level: kind === 'h1' ? 1 : kind === 'h2' ? 2 : 3 })
        .run();
    case 'bullet':
      return editor.isActive('bulletList') ? false : chain.toggleBulletList().run();
    case 'ordered':
      return editor.isActive('orderedList') ? false : chain.toggleOrderedList().run();
    case 'task':
      return editor.isActive('taskList') ? false : chain.toggleTaskList().run();
    case 'quote':
      return editor.isActive('blockquote') ? false : chain.toggleBlockquote().run();
    case 'code':
      return editor.isActive('codeBlock') ? false : chain.toggleCodeBlock().run();
  }
}

export function blockKindLabel(kind: BlockKind): string {
  switch (kind) {
    case 'paragraph':
      return m.documents_kind_paragraph();
    case 'h1':
      return m.documents_kind_h1();
    case 'h2':
      return m.documents_kind_h2();
    case 'h3':
      return m.documents_kind_h3();
    case 'bullet':
      return m.documents_kind_bullet();
    case 'ordered':
      return m.documents_kind_ordered();
    case 'task':
      return m.documents_kind_task();
    case 'quote':
      return m.documents_kind_quote();
    case 'code':
      return m.documents_kind_code();
  }
}

/** / 메뉴 검색어. 화면 언어와 상관없이 한국어·영어·Markdown 표기로 찾는다. */
export function blockKindKeywords(kind: BlockKind): string[] {
  switch (kind) {
    case 'paragraph':
      return ['text', 'paragraph', '문단', '본문', '텍스트'];
    case 'h1':
      return ['h1', 'heading', 'title', '제목', '#'];
    case 'h2':
      return ['h2', 'heading', 'subtitle', '제목', '소제목', '##'];
    case 'h3':
      return ['h3', 'heading', '제목', '소제목', '###'];
    case 'bullet':
      return ['bullet', 'list', 'ul', '목록', '글머리', '-'];
    case 'ordered':
      return ['number', 'ordered', 'list', 'ol', '번호', '목록', '1.'];
    case 'task':
      return ['todo', 'task', 'check', 'checklist', '할일', '할 일', '체크', '체크리스트', '[]'];
    case 'quote':
      return ['quote', 'blockquote', '인용', '>'];
    case 'code':
      return ['code', 'codeblock', '코드', '```'];
  }
}

export function blockKindHint(kind: BlockKind): string {
  switch (kind) {
    case 'paragraph':
      return m.documents_hint_paragraph();
    case 'h1':
      return m.documents_hint_h1();
    case 'h2':
      return m.documents_hint_h2();
    case 'h3':
      return m.documents_hint_h3();
    case 'bullet':
      return m.documents_hint_bullet();
    case 'ordered':
      return m.documents_hint_ordered();
    case 'task':
      return m.documents_hint_task();
    case 'quote':
      return m.documents_hint_quote();
    case 'code':
      return m.documents_hint_code();
  }
}
