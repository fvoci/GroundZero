import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api, NetworkError, type DocumentView } from '$lib/api/client';
import { documentsApi } from '$lib/features/documents/api';
import DocumentActions from '$lib/features/documents/DocumentActions.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';
import { ACCESS_DOCUMENT } from '../../stories/fixtures/access';
import { unloadBlocked } from './fixtures/leave-warning';
import { permissionState } from './fixtures/permission-state.svelte';

// #289: 공유 이메일 폼은 공유를 관리할 수 있을 때(`canManage`)만 보인다. 관리할 수 없게 되면 숨은 이메일이 떠날 때
// 경고를 남기면 안 된다. 공유 설정은 같은 창에 계속 보이므로 그 미저장 입력은 그대로 보호한다(D-27, D-113).

const document: DocumentView = { ...ACCESS_DOCUMENT, visibility: 'workspace' };

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  localStorage.clear();
  me.userId = 'user-a';
  vi.spyOn(api, 'shares').mockResolvedValue([]);
  vi.spyOn(api, 'publicLinks').mockResolvedValue([]);
  vi.spyOn(api, 'document').mockResolvedValue(document);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  me.userId = '';
  localStorage.clear();
});

async function openShare(doc: DocumentView): Promise<void> {
  render(DocumentActions, { doc, workspaces: [], onchanged: () => undefined });
  await fireEvent.click(screen.getByRole('button', { name: m.documents_share_settings() }));
  await screen.findByLabelText(m.documents_email());
}

it('drops the hidden sharing email when share management ends while the dialog stays open', async () => {
  const share = vi.spyOn(api, 'share');
  const doc = permissionState({ ...document });
  await openShare(doc);
  await fireEvent.input(screen.getByLabelText(m.documents_email()), {
    target: { value: 'reader@example.test' },
  });
  expect(unloadBlocked()).toBe(true);
  doc.access = 'write';
  doc.can_manage = false;
  await waitFor(() => {
    expect(screen.queryByLabelText(m.documents_email())).toBeNull();
  });
  // 공유 설정 창은 계속 열려 있다.
  expect(screen.getByRole('dialog')).toBeInTheDocument();
  expect(unloadBlocked()).toBe(false);
  expect(router.guard?.(vi.fn(), '/')).toBe(true);
  doc.access = 'owner';
  doc.can_manage = true;
  expect(await screen.findByLabelText(m.documents_email())).toHaveValue('');
  expect(screen.queryByRole('button', { name: m.common_draft_restore() })).toBeNull();
  expect(share).not.toHaveBeenCalled();
});

it('keeps guarding visible pending share settings after the email is dropped', async () => {
  const save = vi.spyOn(documentsApi, 'setSettings').mockResolvedValue({
    kind: 'conflict',
    current: { ...document, visibility: 'private' },
    changed: [],
    requestId: 'conflict',
  });
  const doc = permissionState({ ...document });
  await openShare(doc);
  await fireEvent.input(screen.getByLabelText(m.documents_email()), {
    target: { value: 'reader@example.test' },
  });
  await fireEvent.click(await screen.findByRole('checkbox', { name: /^비공개/ }));
  await screen.findByRole('region', { name: '지금과 비교' });
  doc.access = 'write';
  doc.can_manage = false;
  await waitFor(() => {
    expect(screen.queryByLabelText(m.documents_email())).toBeNull();
  });
  // 충돌 뒤 남긴 설정 입력은 계속 보이므로 경고가 남는다.
  expect(unloadBlocked()).toBe(true);
  save.mockResolvedValue({ kind: 'saved', document: { ...document, visibility: 'private' } });
  await fireEvent.click(screen.getByRole('button', { name: m.common_retry() }));
  await waitFor(() => {
    expect(unloadBlocked()).toBe(false);
  });
});

it('keeps a sent share locked until its response, then drops the hidden email', async () => {
  let reject: (e: Error) => void = () => undefined;
  vi.spyOn(api, 'share').mockReturnValue(
    new Promise((_, fail) => {
      reject = fail;
    }),
  );
  const doc = permissionState({ ...document });
  await openShare(doc);
  await fireEvent.input(screen.getByLabelText(m.documents_email()), {
    target: { value: 'reader@example.test' },
  });
  await fireEvent.click(screen.getByRole('button', { name: m.documents_share() }));
  doc.access = 'write';
  doc.can_manage = false;
  await waitFor(() => {
    expect(screen.queryByLabelText(m.documents_email())).toBeNull();
  });
  expect(router.guard?.(vi.fn(), '/')).toBe(false);
  reject(new NetworkError('offline'));
  await waitFor(() => {
    expect(unloadBlocked()).toBe(false);
  });
  expect(router.guard?.(vi.fn(), '/')).toBe(true);
});
