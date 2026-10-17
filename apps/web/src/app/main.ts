import { mount } from 'svelte';

import { browserLocale } from '$lib/display';
import { setLocale } from '$lib/paraglide/runtime.js';

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

mount(App, { target });
