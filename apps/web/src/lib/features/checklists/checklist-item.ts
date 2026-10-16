// 편집기 밖 체크리스트 (D-30, O-04). 업무 체크리스트는 서버의 행이 원본이다.

export interface ChecklistItem {
  id: string;
  text: string;
  done: boolean;
}

/** 붙여넣은 Markdown에서 펼친 항목. 사용자가 확인·수정한 뒤 덧붙인다. */
export interface ParsedChecklistItem {
  text: string;
  done: boolean;
}

const CHECKBOX = /^\s*[-*+]\s+\[( |x|X)\]\s+(.+?)\s*$/;
const BULLET = /^\s*[-*+]\s+(.+?)\s*$/;

/**
 * `- [x] 자료 수집`은 완료, `- [ ] 초안 작성`은 미완료로 펼친다. 체크 상자가 없는 목록 줄(`- 메모`)은
 * 미완료 항목으로 둔다. 그 밖의 줄(빈 줄·제목·문단)은 무시한다.
 */
export function parseMarkdownChecklist(markdown: string): ParsedChecklistItem[] {
  const items: ParsedChecklistItem[] = [];
  for (const line of markdown.split(/\r?\n/)) {
    const box = CHECKBOX.exec(line);
    if (box?.[2] !== undefined) {
      items.push({ text: box[2], done: box[1] !== ' ' });
      continue;
    }
    const bullet = BULLET.exec(line);
    if (bullet?.[1] !== undefined) items.push({ text: bullet[1], done: false });
  }
  return items;
}

/** 덧붙일 항목. 재시도해도 같은 항목이 두 번 생기지 않도록 요청 id를 미리 붙인다. */
export interface NewChecklistEntry extends ParsedChecklistItem {
  requestId: string;
}
