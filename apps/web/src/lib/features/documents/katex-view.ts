import type { KatexOptions } from 'katex';
import { m } from '$lib/paraglide/messages.js';
import { loadKatex } from './katex-loader';

const views = new WeakMap<HTMLElement, object>();

export function render(latex: string, element: HTMLElement, options?: KatexOptions): void {
  const token = {};
  views.set(element, token);
  const source = document.createElement('span');
  source.textContent = latex;
  const status = document.createElement('span');
  status.setAttribute('role', 'status');
  status.setAttribute('aria-live', 'polite');
  status.className = 'text-sm text-destructive empty:hidden';
  const actions = document.createElement('span');
  actions.hidden = true;
  const retry = document.createElement('button');
  retry.type = 'button';
  // Tiptap owns this native NodeView; use the existing semantic color/focus tokens.
  retry.className = 'text-primary underline focus-visible:outline-2 focus-visible:outline-ring';
  retry.textContent = m.common_retry();
  actions.append(retry);
  element.replaceChildren(source, status, actions);
  let loading = false;
  const current = (): boolean => element.isConnected && views.get(element) === token;
  const draw = async (): Promise<void> => {
    if (loading || !current()) return;
    loading = true;
    retry.disabled = true;
    status.textContent = '';
    try {
      const math = await loadKatex();
      if (!current()) return;
      math.render(latex, source, options);
      actions.hidden = true;
    } catch {
      if (!current()) return;
      source.textContent = latex;
      status.textContent = ` ${m.documents_math_render_failed()} `;
      actions.hidden = false;
    } finally {
      loading = false;
      retry.disabled = false;
    }
  };
  retry.addEventListener('click', (event) => {
    event.preventDefault();
    event.stopPropagation();
    void draw();
  });
  // NodeViews are attached after render() returns. Serialization-only editors
  // stay detached and must not fetch assets merely to parse/save a math node.
  queueMicrotask(() => {
    void draw();
  });
}

export default { render };
