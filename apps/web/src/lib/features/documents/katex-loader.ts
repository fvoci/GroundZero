import type { KatexOptions } from 'katex';
import cssUrl from '../../../styles/katex-layout.css?url';
import { registerPreloadErrorHandler } from '$lib/features/common/preload-reload';

export interface MathRenderer {
  render(latex: string, element: HTMLElement, options?: KatexOptions): void;
}

let pending: Promise<MathRenderer> | undefined;
let style: Promise<void> | undefined;
let failedModuleUrl: URL | undefined;
let retry = 0;
let styleAttempts = 0;

function loadStyle(): Promise<void> {
  style ??= new Promise<void>((resolve, reject) => {
    const link = document.createElement('link');
    link.rel = 'stylesheet';
    const url = new URL(cssUrl, location.href);
    if (styleAttempts++ > 0) url.searchParams.set('fvoci_math_retry', String(styleAttempts));
    link.href = url.href;
    link.onload = () => {
      resolve();
    };
    link.onerror = () => {
      link.remove();
      style = undefined;
      reject(new Error('Math stylesheet unavailable'));
    };
    document.head.append(link);
  });
  return style;
}

function renderer(value: unknown): value is MathRenderer {
  return (
    typeof value === 'object' &&
    value !== null &&
    'render' in value &&
    typeof value.render === 'function'
  );
}

async function importRenderer(): Promise<MathRenderer> {
  // This optional renderer has a local retry UI. Do not let the global stale-chunk
  // handler reload the editor on its failure. Other chunks retain that handler.
  const dispose = registerPreloadErrorHandler((payload) => {
    if (!(payload instanceof Error)) return false;
    for (const text of payload.message.match(/https?:\/\/[^\s"'<>)]*/g) ?? []) {
      if (!URL.canParse(text)) continue;
      const url = new URL(text);
      if (
        url.origin !== location.origin ||
        !/\/katex(?:-(?!layout-)[^/]+)?\.js$/.test(url.pathname)
      )
        continue;
      failedModuleUrl = url;
      return true;
    }
    return false;
  });
  try {
    let module: unknown;
    if (failedModuleUrl === undefined) module = await import('katex');
    else {
      // A failed module URL can stay in the browser's module map. Retry only the
      // same-origin KaTeX asset with a fresh URL, retaining its existing imports.
      const url = new URL(failedModuleUrl);
      url.searchParams.set('fvoci_math_retry', String(++retry));
      module = await import(/* @vite-ignore */ url.href);
    }
    if (
      typeof module === 'object' &&
      module !== null &&
      'default' in module &&
      renderer(module.default)
    )
      return module.default;
    throw new Error('Math renderer unavailable');
  } finally {
    dispose();
  }
}

/** Shared renderer assets only; no document/account data is cached here. */
export function loadKatex(): Promise<MathRenderer> {
  pending ??= Promise.all([importRenderer(), loadStyle()])
    .then(([math]) => math)
    .catch((error: unknown) => {
      pending = undefined;
      throw error;
    });
  return pending;
}
