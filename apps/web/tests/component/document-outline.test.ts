import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import DocumentOutline from '$lib/features/documents/DocumentOutline.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 위키(목표 4): 읽기 화면의 목차. 본문 제목(h1–h3)을 차례대로 보이고, 누르면 그 제목으로 간다. 본문이 바뀌면
// (입력·공동 편집) 따라 바뀐다. 제목이 둘보다 적으면 보이지 않는다.
describe('document outline', () => {
  it('lists headings, follows edits and scrolls to a heading', async () => {
    await setLocale('ko', { reload: false });
    const root = document.createElement('div');
    root.innerHTML = '<div class="ProseMirror"><h1>개요</h1><p>본문</p><h2>설치</h2></div>';
    document.body.append(root);
    const scrolled = vi.fn();
    Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', {
      configurable: true,
      value: scrolled,
    });
    render(DocumentOutline, { props: { root } });
    const nav = await screen.findByRole('navigation', { name: '목차' });
    expect([...nav.querySelectorAll('button')].map((b) => b.textContent.trim())).toEqual([
      '개요',
      '설치',
    ]);

    root.querySelector('.ProseMirror')?.insertAdjacentHTML('beforeend', '<h3>문제 해결</h3>');
    await waitFor(() => {
      expect(screen.getAllByRole('button').map((b) => b.textContent.trim())).toContain('문제 해결');
    });
    await fireEvent.click(screen.getByRole('button', { name: '설치' }));
    expect(scrolled).toHaveBeenCalled();
    expect(scrolled.mock.contexts[0]).toBe(root.querySelector('h2'));
    root.remove();
  });
});
