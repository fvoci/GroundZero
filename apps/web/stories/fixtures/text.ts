// 표시가 깨지기 쉬운 문구(긴 한국어, 공백 없는 문자열, 숫자, 이모지). 개발 seed와 같은 합성 데이터에서 가져온다.
import { createRawSnippet, type Snippet } from 'svelte';

import { SAMPLES } from './synthetic';

export const LONG_KO = SAMPLES.long_ko;
export const NO_SPACE = SAMPLES.no_space;
export const NUMBERS = SAMPLES.numbers;
export const EMOJI = SAMPLES.emoji;

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
