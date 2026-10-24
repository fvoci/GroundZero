import { readFileSync } from 'node:fs';
import path from 'node:path';
import { runInNewContext } from 'node:vm';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ThemeChoice } from '$lib/theme.svelte';
import {
  applyTheme,
  appliedTheme,
  followSystemTheme,
  setTheme,
  storedTheme,
  THEME_KEY,
  themeState,
} from '$lib/theme.svelte';

// 화면 밝기: 시스템 설정 따르기(기본)·밝게·어둡게, 이 브라우저에만 저장. 첫 그림은 index.html의 인라인 스크립트가 정하므로
// 그 스크립트와 applyTheme이 같은 결과를 내는지도 본다(어긋나면 앱이 뜨는 순간 밝기가 바뀌어 번쩍인다).

let systemDark = false;
let listeners: (() => void)[] = [];

function stubSystem(): void {
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: query === '(prefers-color-scheme: dark)' && systemDark,
    media: query,
    addEventListener: (_type: string, fn: () => void) => {
      listeners.push(fn);
    },
    removeEventListener: (_type: string, fn: () => void) => {
      listeners = listeners.filter((l) => l !== fn);
    },
  }));
}

function reset(): void {
  const root = document.documentElement;
  root.classList.remove('dark');
  delete root.dataset.theme;
  window.localStorage.clear();
}

beforeEach(() => {
  systemDark = false;
  listeners = [];
  stubSystem();
  reset();
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  reset();
});

const isDark = (): boolean => document.documentElement.classList.contains('dark');

describe('theme choice', () => {
  it('follows the system by default and stores only an explicit choice', () => {
    expect(storedTheme()).toBe('system');
    systemDark = true;
    applyTheme(storedTheme());
    expect(isDark()).toBe(true);
    expect(themeState.dark).toBe(true);
    expect(appliedTheme()).toBe('system');

    setTheme('light');
    expect(isDark()).toBe(false);
    expect(window.localStorage.getItem(THEME_KEY)).toBe('light');
    expect(appliedTheme()).toBe('light');

    setTheme('system');
    expect(window.localStorage.getItem(THEME_KEY)).toBeNull();
    expect(isDark()).toBe(true);
  });

  it('follows a change of the system setting only while following the system', () => {
    const stop = followSystemTheme();
    applyTheme('system');
    expect(isDark()).toBe(false);
    systemDark = true;
    for (const l of listeners) l();
    expect(isDark()).toBe(true);

    applyTheme('light');
    systemDark = false;
    for (const l of listeners) l();
    systemDark = true;
    for (const l of listeners) l();
    expect(isDark()).toBe(false);
    stop();
    expect(listeners).toHaveLength(0);
  });

  it('applies the theme while storage is unavailable', () => {
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    setTheme('dark');
    expect(isDark()).toBe(true);
  });
});

describe('first paint script in index.html', () => {
  const html = readFileSync(path.resolve(import.meta.dirname, '../../index.html'), 'utf8');
  const inline = /<script>([\s\S]*?)<\/script>/.exec(html)?.[1];

  const cases: [ThemeChoice, boolean][] = [
    ['system', false],
    ['system', true],
    ['light', true],
    ['dark', false],
  ];
  it.each(cases)('agrees with applyTheme for %s (system dark: %s)', (choice, dark) => {
    if (inline === undefined) throw new Error('no inline script in index.html');
    systemDark = dark;
    if (choice !== 'system') window.localStorage.setItem(THEME_KEY, choice);
    // 페이지의 전역(localStorage·matchMedia·document)으로 실행한다. jsdom은 넣은 <script>를 돌리지 않는다.
    runInNewContext(inline, {
      localStorage: window.localStorage,
      matchMedia: (query: string) => window.matchMedia(query),
      document,
    });
    const fromScript = { dark: isDark(), theme: document.documentElement.dataset.theme };
    reset();
    applyTheme(choice);
    expect(fromScript).toEqual({ dark: isDark(), theme: document.documentElement.dataset.theme });
  });
});
