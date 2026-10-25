import type { Workspace } from '@fvoci/api-client';
import { render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocumentView, SummaryView } from '$lib/api/client';
import { api, NetworkError } from '$lib/api/client';
import ChildDocuments from '$lib/features/documents/ChildDocuments.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 문서 아래 하위 문서 목록: 같은 자리 목록을 부모로 거르고, 읽을 수 없는 자리에서는 묻지 않는다.
function view(over: Partial<DocumentView>): DocumentView {
  return {
    id: 'guide',
    workspace_id: 'ws-1',
    placement: 'wiki',
    item_id: null,
    parent_id: null,
    reference: null,
    title: '운영 안내',
    body: { type: 'doc', content: [] },
    schema_version: 1,
    revision: 1,
    updated_at: '2026-10-21T00:00:00Z',
    access: 'write',
    can_edit: true,
    visibility: 'workspace',
    allow_copy: true,
    can_manage: false,
    can_delete: false,
    is_template: false,
    realtime: false,
    can_export: true,
    ...over,
  };
}
function summary(id: string, title: string, parent: string | null): SummaryView {
  return { id, title, parent_id: parent, reference: null, revision: 1, updated_at: '' };
}
const workspaces: Workspace[] = [
  {
    id: 'ws-1',
    name: '모듈 예제',
    organization_id: 'org-1',
    role: 'member',
    is_owner: false,
    realtime_enabled: false,
  },
];
const list = [
  summary('guide', '운영 안내', null),
  summary('install', '설치', 'guide'),
  summary('backup', '백업', 'install'),
  summary('faq', '자주 묻는 질문', 'guide'),
];

describe('child documents', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('lists only the direct children, in list order, linking to each', async () => {
    vi.spyOn(api, 'wikiDocuments').mockResolvedValue(list);
    render(ChildDocuments, { props: { doc: view({}), workspaces } });
    const nav = await screen.findByRole('navigation', { name: '하위 문서 2개' });
    const links = within(nav).getAllByRole('link');
    expect(links.map((a) => a.textContent.trim())).toEqual(['설치', '자주 묻는 질문']);
    expect(links[0]?.getAttribute('href')).toContain('install');
  });

  it('shows nothing when there are no children', async () => {
    const read = vi.spyOn(api, 'wikiDocuments').mockResolvedValue(list);
    render(ChildDocuments, { props: { doc: view({ id: 'backup' }), workspaces } });
    await waitFor(() => {
      expect(read).toHaveBeenCalled();
    });
    expect(screen.queryByRole('navigation')).toBeNull();
  });

  it('uses the item documents and my notes for those places, and asks nothing for a wiki it cannot read', async () => {
    const item = vi.spyOn(api, 'itemDocuments').mockResolvedValue(list);
    const notes = vi.spyOn(api, 'myNotes').mockResolvedValue(list);
    const wiki = vi.spyOn(api, 'wikiDocuments').mockResolvedValue(list);
    const first = render(ChildDocuments, {
      props: { doc: view({ item_id: 'it-1', placement: 'module' }), workspaces },
    });
    await screen.findByRole('navigation');
    expect(item).toHaveBeenCalledWith('it-1');
    first.unmount();
    const second = render(ChildDocuments, {
      props: {
        doc: view({ workspace_id: null, placement: 'personal', access: 'owner' }),
        workspaces,
      },
    });
    await screen.findByRole('navigation');
    expect(notes).toHaveBeenCalled();
    second.unmount();
    render(ChildDocuments, { props: { doc: view({ workspace_id: 'other-ws' }), workspaces } });
    expect(wiki).not.toHaveBeenCalled();
    expect(screen.queryByRole('navigation')).toBeNull();
  });

  // 375px에서 긴 제목 옆의 `WIKI-9`가 `WI / KI / -9`로 갈라졌다. 배치는 jsdom이 계산하지 않아 구조만 지킨다.
  it('keeps the reference on one line and lets a long title break instead', async () => {
    const url =
      'https://example.test/workspaces/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/documents/ffffffff';
    vi.spyOn(api, 'wikiDocuments').mockResolvedValue([
      summary('guide', '운영 안내', null),
      { ...summary('long', url, 'guide'), reference: 'WIKI-9' },
    ]);
    render(ChildDocuments, { props: { doc: view({}), workspaces } });
    const nav = await screen.findByRole('navigation');
    const reference = within(nav).getByText('WIKI-9');
    expect(reference).toHaveClass('shrink-0', 'whitespace-nowrap');
    expect(within(nav).getByText(url)).toHaveClass('min-w-0', '[overflow-wrap:anywhere]');
  });

  it('says so when the list cannot be loaded', async () => {
    vi.spyOn(api, 'wikiDocuments').mockRejectedValue(new NetworkError('offline'));
    render(ChildDocuments, { props: { doc: view({}), workspaces } });
    expect(await screen.findByText('하위 문서를 불러오지 못했습니다.')).toBeTruthy();
  });
});

describe('child documents after a save', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('does not read the list again when only the document object changes (same place)', async () => {
    await setLocale('ko', { reload: false });
    const read = vi.spyOn(api, 'wikiDocuments').mockResolvedValue(list);
    const view = render(ChildDocuments, { props: { doc: view0(), workspaces } });
    await screen.findByRole('navigation');
    // 저장하면 화면이 새 문서 객체(리비전·제목 등)를 넘긴다.
    await view.rerender({
      doc: { ...view0(), revision: 2, title: '운영 안내 (고침)' },
      workspaces,
    });
    await screen.findByRole('navigation');
    expect(read).toHaveBeenCalledTimes(1);
  });
});

function view0(): DocumentView {
  return view({});
}
