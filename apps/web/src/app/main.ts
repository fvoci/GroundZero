import { mount } from 'svelte';

import { browserLocale } from '$lib/display';
import { applyDocFont, storedDocFont } from '$lib/doc-font';
import { keepFocusAcrossLocks } from '$lib/features/common/lock-focus';
import { setLocale } from '$lib/paraglide/runtime.js';
import { applyTheme, followSystemTheme, storedTheme } from '$lib/theme.svelte';

import App from './App.svelte';
import '../styles/app.css';
// 수식 배치(KaTeX). 글꼴은 O-30 결정 전까지 넣지 않는다.
import '../styles/katex-layout.css';

const target = document.getElementById('app');
if (target === null) {
  throw new Error('mount target #app not found');
}

// 처음 그리기 전 표시 언어(D-81): 로그인 전에는 브라우저 언어. 로그인하면 App이 저장한 설정으로 바꾼다.
await setLocale(browserLocale(), { reload: false });
document.documentElement.lang = browserLocale();
// 이 브라우저에서 고른 문서 본문 서체(D-86).
applyDocFont(storedDocFont());
// 이 브라우저에서 고른 화면 밝기. 첫 그림은 index.html이 정했고, 여기서 상태를 맞추고 시스템 설정 변화를 따른다.
applyTheme(storedTheme());
followSystemTheme();
// 저장 중 잠금(fieldset disabled)이 풀리면 Enter로 제출한 입력에 초점을 되돌린다(R5 2차 delta 4묶음 확인 필요 1).
keepFocusAcrossLocks();

mount(App, { target });
