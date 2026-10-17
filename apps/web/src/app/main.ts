import { mount } from 'svelte';

import App from './App.svelte';
import '../styles/app.css';
// 수식 배치(KaTeX). 글꼴은 O-30 결정 전까지 넣지 않는다.
import '../styles/katex-layout.css';

const target = document.getElementById('app');
if (target === null) {
  throw new Error('mount target #app not found');
}

mount(App, { target });
