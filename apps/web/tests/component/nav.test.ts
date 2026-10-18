import { fireEvent, render, screen } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { SummaryView } from '$lib/api/client';
import { api } from '$lib/api/client';
import SidebarDocTree from '$lib/features/nav/SidebarDocTree.svelte';
import { workspaceNav } from '$lib/features/nav/workspace-nav.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { MenuButton } from '$lib/ui/sidebar';

function doc(id: string, title: string, parent: string | null): SummaryView {
  return {
    id,
    title,
    parent_id: parent,
    reference: null,
    revision: 1,
    updated_at: '2026-10-18T00:00:00Z',
  };
}

// 가
//   가-1
//     가-1-ㄱ
// 나
const docs = [
  doc('a', '가', null),
  doc('b', '나', null),
  doc('a1', '가-1', 'a'),
  doc('a1x', '가-1-ㄱ', 'a1'),
];

describe('sidebar document tree', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });

  it('opens the ancestors of the current document and marks it current', () => {
    render(SidebarDocTree, { workspaceId: 'w', docs, activeId: 'a1x' });
    const current = screen.getByRole('link', { name: '가-1-ㄱ' });
    expect(current.getAttribute('aria-current')).toBe('page');
    expect(screen.getByRole('link', { name: '가' }).getAttribute('aria-current')).toBeNull();
    expect(
      screen.getByRole('button', { name: '가 하위 문서 접기' }).getAttribute('aria-expanded'),
    ).toBe('true');
  });

  it('collapses and expands a branch with its button', async () => {
    render(SidebarDocTree, { workspaceId: 'w', docs, activeId: undefined });
    expect(screen.queryByRole('link', { name: '가-1' })).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: '가 하위 문서 펼치기' }));
    expect(screen.getByRole('link', { name: '가-1' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: '가 하위 문서 접기' }));
    expect(screen.queryByRole('link', { name: '가-1' })).toBeNull();
    // 하위가 없는 문서에는 펼치기 버튼이 없다.
    expect(screen.queryByRole('button', { name: /나 하위 문서/ })).toBeNull();
  });
});

describe('shared workspace navigation lists', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('asks the server once per workspace and drops answers for a workspace left behind', async () => {
    let resolveSlow: (list: SummaryView[]) => void = () => undefined;
    const modules = vi.spyOn(api, 'modules').mockResolvedValue([]);
    const wiki = vi
      .spyOn(api, 'wikiDocuments')
      .mockImplementationOnce(
        () =>
          new Promise<SummaryView[]>((resolve) => {
            resolveSlow = resolve;
          }),
      )
      .mockResolvedValue([doc('n', '새 문서', null)]);

    workspaceNav.show('ws-old');
    workspaceNav.show('ws-old');
    expect(modules).toHaveBeenCalledTimes(1);
    expect(wiki).toHaveBeenCalledTimes(1);

    workspaceNav.show('ws-new');
    expect(wiki).toHaveBeenCalledTimes(2);
    await vi.waitFor(() => {
      expect(workspaceNav.docs.state).toBe('ready');
    });
    resolveSlow([doc('old', '옛 문서', null)]);
    await Promise.resolve();
    expect(workspaceNav.docs).toEqual({ state: 'ready', list: [doc('n', '새 문서', null)] });
  });
});

describe('sidebar menu button', () => {
  it('sets data-active only on the active entry (Tailwind data-active: matches any value)', () => {
    const children = createRawSnippet(() => ({ render: () => '<span>항목</span>' }));
    const { container } = render(MenuButton, { isActive: false, children });
    expect(container.querySelector('[data-sidebar=menu-button]')?.hasAttribute('data-active')).toBe(
      false,
    );
    const active = render(MenuButton, { isActive: true, children });
    expect(
      active.container.querySelector('[data-sidebar=menu-button]')?.hasAttribute('data-active'),
    ).toBe(true);
  });
});
