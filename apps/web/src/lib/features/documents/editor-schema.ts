// 문서 편집기의 스키마를 정하는 확장 목록. 편집기(TiptapEditor)와 실시간 공동 편집의 문서 형식 검사(테스트)가 같은
// 목록을 쓴다. 스키마가 바뀌면 서버의 Yjs ↔ JSON 변환 기준(contracts/collab)도 다시 만든다.
import type { Extensions } from '@tiptap/core';
import type { Node as PmNode } from '@tiptap/pm/model';
import Image from '@tiptap/extension-image';
import { TaskItem, TaskList } from '@tiptap/extension-list';
import Mathematics from '@tiptap/extension-mathematics';
import { TableKit } from '@tiptap/extension-table';
import { Markdown } from '@tiptap/markdown';
import StarterKit from '@tiptap/starter-kit';

import type { ReferenceOptions } from '$lib/features/references/reference-extension';
import { Reference } from '$lib/features/references/reference-extension';
import { m } from '$lib/paraglide/messages.js';

export interface SchemaOptions {
  reference?: Partial<ReferenceOptions>;
  onMathClick?: (kind: 'inline' | 'block', node: PmNode, pos: number) => void;
  /** 실시간 공동 편집이면 Yjs가 되돌리기를 맡으므로 기본 되돌리기 기록을 끈다. */
  collaborative?: boolean;
}

export function schemaExtensions(options: SchemaOptions = {}): Extensions {
  const { reference = {}, onMathClick, collaborative = false } = options;
  return [
    StarterKit.configure(collaborative ? { undoRedo: false } : {}),
    TaskList,
    TaskItem.configure({
      nested: true,
      a11y: {
        checkboxLabel: (node, checked) => {
          const text = node.textContent || m.documents_task_empty();
          return checked
            ? m.documents_task_checkbox_done({ text })
            : m.documents_task_checkbox_open({ text });
        },
      },
    }),
    Markdown,
    Reference.configure(reference),
    TableKit.configure({ table: { resizable: false } }),
    // 기본 `![](주소)` 입력 규칙은 주소를 가리지 않아 끈다(편집할 때 SafeImageInput이 대신한다). 스키마는 같다.
    Image.extend({
      addInputRules() {
        return [];
      },
    }).configure({ inline: false, allowBase64: false }),
    Mathematics.configure({
      katexOptions: { throwOnError: false },
      inlineOptions: {
        onClick: (node, pos) => {
          onMathClick?.('inline', node, pos);
        },
      },
      blockOptions: {
        onClick: (node, pos) => {
          onMathClick?.('block', node, pos);
        },
      },
    }),
  ];
}
