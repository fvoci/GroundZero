import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import type { DocumentView } from '$lib/api/client';
import DocumentEditor from '$lib/features/documents/DocumentEditor.svelte';
import { loadDraft, loadOffers, storeDraft } from '$lib/features/documents/drafts';
import { accountSafety } from '$lib/features/feedback/feedback.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

const documentView: DocumentView = {
  id: 'doc-1',
  workspace_id: 'ws-1',
  placement: 'wiki',
  item_id: null,
  parent_id: null,
  reference: null,
  title: '서버 제목',
  body: { type: 'doc', content: [] },
  schema_version: 1,
  revision: 1,
  updated_at: '2026-10-26T00:00:00Z',
  access: 'write',
  can_edit: true,
  visibility: 'workspace',
  allow_copy: true,
  can_manage: false,
  can_delete: false,
  is_template: false,
  realtime: false,
  can_export: true,
};

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'user-a';
  localStorage.clear();
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
});
afterEach(() => {
  cleanup();
  accountSafety.switching = false;
  vi.useRealTimers();
  vi.restoreAllMocks();
  localStorage.clear();
  me.userId = '';
});

it.each([true, false])(
  'protects the offered draft on account reload when reserved=%s',
  async (reserved) => {
    const oldDraft = {
      baseRevision: 1,
      title: '지난번 초안',
      body: documentView.body,
      savedAt: '2026-10-26T01:00:00Z',
    };
    expect(storeDraft(documentView.id, oldDraft)).toBe(true);
    if (reserved) {
      const setItem = localStorage.setItem.bind(localStorage);
      vi.spyOn(Storage.prototype, 'setItem').mockImplementation((key: string, value: string) => {
        if (key.includes(':offer:')) throw new DOMException('full', 'QuotaExceededError');
        setItem(key, value);
      });
    }
    render(DocumentEditor, { initial: documentView });
    expect(screen.getByRole('region', { name: '보관된 초안' })).toBeInTheDocument();
    const title = document.getElementById('doc-title');
    if (title === null) throw new Error('missing title');
    await fireEvent.input(title, { target: { value: '지금 편집' } });
    accountSafety.switching = true;
    const leaving = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(leaving);

    expect([loadDraft(documentView.id), ...loadOffers(documentView.id)]).toContainEqual(oldDraft);
    expect(leaving.defaultPrevented).toBe(reserved);
    expect(title).toHaveValue('지금 편집');
    if (!reserved) expect(loadDraft(documentView.id)?.title).toBe('지금 편집');
  },
);
