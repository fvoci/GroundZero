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

import { isAllowedHref, isAttachmentUrl } from './paste-guard';

export interface SchemaOptions {
  reference?: Partial<ReferenceOptions>;
  onMathClick?: (kind: 'inline' | 'block', node: PmNode, pos: number) => void;
  /** 실시간 공동 편집이면 Yjs가 되돌리기를 맡으므로 기본 되돌리기 기록을 끈다. */
  collaborative?: boolean;
}

/**
 * 링크 주소 허용. 주소가 없는 링크 표시(서버가 빈 주소로 받는다)는 그리면서 null이 오므로 그대로 둔다
 * (문자열이 아니면 고를 주소가 없다).
 */
function allowedLinkUri(url: unknown): boolean {
  return typeof url !== 'string' || isAllowedHref(url);
}

export function schemaExtensions(options: SchemaOptions = {}): Extensions {
  const { reference = {}, onMathClick, collaborative = false } = options;
  return [
    // 링크는 자동 링크·붙여넣기 규칙도 서버 규칙(isAllowedHref)으로 거른다. 기본은 ftp·tel 등도 링크로 만들어
    // 저장이 거절되거나 실시간 방이 닫힌다. 표시(마크·속성)는 같아 스키마는 그대로다.
    StarterKit.configure({
      ...(collaborative ? { undoRedo: false as const } : {}),
      link: { isAllowedUri: (url, ctx) => ctx.defaultValidate(url) && allowedLinkUri(url) },
    }),
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
    // 편집기 화면에서 바깥 주소 이미지(Markdown 원문·초안 복원으로 들어온 것)는 불러오지 않고, 주소와 함께 저장할 수
    // 없다는 표시로 보인다. 내용은 그대로 남고 저장 전 검사(contentIssues)가 원인과 고칠 방법을 알린다.
    Image.extend({
      addInputRules() {
        return [];
      },
      addNodeView() {
        return ({ node, HTMLAttributes }) => {
          const src = typeof node.attrs.src === 'string' ? node.attrs.src : '';
          if (isAttachmentUrl(src)) {
            const img = document.createElement('img');
            for (const [key, value] of Object.entries(HTMLAttributes))
              if (value !== null && value !== undefined) img.setAttribute(key, String(value));
            return { dom: img };
          }
          const blocked = document.createElement('div');
          blocked.className = 'fvoci-blocked-image';
          blocked.dataset.blockedImage = '';
          blocked.setAttribute('role', 'note');
          blocked.textContent = m.documents_image_blocked({ src });
          return { dom: blocked };
        };
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
