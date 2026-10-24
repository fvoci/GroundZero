// story에서 쓰는 합성 문구와 글 조각. 표시가 깨지기 쉬운 경우(긴 한국어, 공백 없는 문자열, 숫자, 이모지)를 모았다.
import { createRawSnippet, type Snippet } from 'svelte';

export const LONG_KO =
  '2026학년도 2학기 대학원 연구실 공동 세미나 발표 자료와 회의록을 한곳에 모아 정리하는 작업 공간';
export const NO_SPACE =
  'https://example.test/workspaces/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/documents/ffffffff-0000-1111-2222-333333333333';
export const NUMBERS = '12,345,678.90 · 2026-10-25 · 0001234567';
export const EMOJI = '회의 준비 🎉📚✅ 👩‍💻🧑‍🔬';

function escapeHtml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}

/** 자식 글자만 받는 컴포넌트(Button·Badge)에 넘길 글 조각. 스타일 없는 span 하나로 감싼다(Svelte 조각은 요소 하나가 필요). */
export function text(value: string): Snippet {
  return createRawSnippet(() => ({ render: () => `<span>${escapeHtml(value)}</span>` }));
}
