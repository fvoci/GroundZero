import { mount } from 'svelte';

import App from './App.svelte';
import '../styles/app.css';

const target = document.getElementById('app');
if (target === null) {
  throw new Error('mount target #app not found');
}

mount(App, { target });
