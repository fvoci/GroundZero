import type { Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import Backlinks from '$lib/features/references/Backlinks.svelte';
import { parseReferenceHash, referenceHash } from '$lib/features/references/focus';
import ReferenceFocus from '$lib/features/references/ReferenceFocus.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

// 백링크 → 원문 문서 안의 참조 자리: 찾음(한 곳·여러 곳)과 못 찾음(지워짐), 늦게 그려지는 본문.
const ITEM = '01a15a91-0d23-7334-b392-a88947def40d';
const OTHER = '01a15a91-0d23-7334-b392-a8b0283cf9ed';

function body(...refs: string[]): HTMLElement {
  const root = document.createElement('div');
  root.innerHTML = refs
    .map((id) => `<p>앞 글 <a data-reference data-kind="item" data-id="${id}">항목</a> 뒤 글</p>`)
    .join('');
  document.body.append(root);
  return root;
}

describe('reference hash', () => {
  it('round-trips a target and rejects anything else', () => {
    const hash = referenceHash({ kind: 'item', id: ITEM });
    expect(hash).toBe(`#ref-item-${ITEM}`);
    expect(parseReferenceHash(hash)).toEqual({ kind: 'item', id: ITEM });
    expect(parseReferenceHash('#ref-user-' + ITEM)).toBeUndefined();
    expect(parseReferenceHash('#heading')).toBeUndefined();
  });
});

// 알림 영역은 늘 있고 보일 때만 글이 들어간다.
const statusText = (): string => screen.getByRole('status').textContent.trim();

const scroll = vi.fn<(arg?: boolean | ScrollIntoViewOptions) => void>();

describe('reference focus', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    Element.prototype.scrollIntoView = scroll;
  });
  afterEach(() => {
    vi.restoreAllMocks();
    document.body.innerHTML = '';
  });

  it('scrolls to the reference and steps through several', async () => {
    const root = body(ITEM, OTHER, ITEM);
    render(ReferenceFocus, { props: { root, target: { kind: 'item', id: ITEM } } });
    await waitFor(() => {
      expect(statusText()).toContain('2곳 가운데 1번째');
    });
    const refs = [...root.querySelectorAll<HTMLElement>(`[data-id="${ITEM}"]`)];
    expect(refs[0]?.classList.contains('ring-2')).toBe(true);
    expect(scroll).toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: '다음 참조' }));
    expect(statusText()).toContain('2곳 가운데 2번째');
    expect(refs[1]?.classList.contains('ring-2')).toBe(true);
    expect(refs[0]?.classList.contains('ring-2')).toBe(false);
  });

  it('finds a reference drawn after the screen opened', async () => {
    const root = body();
    render(ReferenceFocus, { props: { root, target: { kind: 'item', id: ITEM } } });
    root.append(body(ITEM));
    await waitFor(() => {
      expect(statusText()).toContain('1곳 가운데 1번째');
    });
    expect(screen.queryByRole('button', { name: '다음 참조' })).toBeNull();
  });

  it('says it could not find the reference when it is gone, and does nothing without a target', async () => {
    const root = body(OTHER);
    render(ReferenceFocus, { props: { root, target: { kind: 'item', id: ITEM }, wait: 20 } });
    await waitFor(() => {
      expect(statusText()).toContain('찾지 못했습니다');
    });
    await fireEvent.click(screen.getByRole('button', { name: '닫기' }));
    expect(statusText()).toBe('');
  });
});

describe('backlinks point at the reference', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('links to the source document with the target in the address', async () => {
    const workspaces: Workspace[] = [
      {
        id: 'ws-1',
        name: '공간',
        organization_id: 'org-1',
        role: 'member',
        is_owner: false,
        realtime_enabled: false,
      },
    ];
    vi.spyOn(api, 'itemBacklinks').mockResolvedValue([
      { id: 'log', reference: null, title: '점검 기록', workspace_id: 'ws-1', item_id: null },
    ]);
    const navigate = vi.spyOn(router, 'navigate').mockImplementation(() => undefined);
    render(Backlinks, { props: { kind: 'item', id: ITEM, workspaces } });
    const link = await screen.findByRole('link', { name: /점검 기록/ });
    expect(link.getAttribute('href')).toMatch(new RegExp(`/log#ref-item-${ITEM}$`));
    await fireEvent.click(link);
    await waitFor(() => {
      expect(navigate).toHaveBeenCalledWith(expect.stringMatching(`#ref-item-${ITEM}$`));
    });
  });
});

describe('router keeps the # apart from the path', () => {
  afterEach(() => {
    router.guard = undefined;
  });

  it('splits it off for routing and does not ask the unsaved guard for a #-only change', () => {
    router.navigate(`/w/ws-1/documents/log#ref-item-${ITEM}`);
    expect(router.path).toBe('/w/ws-1/documents/log');
    expect(router.hash).toBe(`#ref-item-${ITEM}`);
    const guard = vi.fn(() => false);
    router.guard = guard;
    router.navigate('/w/ws-1/documents/log');
    expect(guard).not.toHaveBeenCalled();
    expect(router.hash).toBe('');
    router.navigate('/w/ws-1/documents/other');
    expect(guard).toHaveBeenCalledTimes(1);
    expect(router.path).toBe('/w/ws-1/documents/log');
  });
});

describe('reference focus before the body exists', () => {
  it('waits without failing while the body area is not drawn yet (null)', async () => {
    await setLocale('ko', { reload: false });
    const view = render(ReferenceFocus, {
      props: { root: null, target: { kind: 'item', id: ITEM } },
    });
    expect(statusText()).toBe('');
    view.unmount();
  });
});
