import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { FileView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import AttachmentsPanel from '$lib/features/files/AttachmentsPanel.svelte';
import { notifyDone } from '$lib/features/feedback/feedback.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { me } from '$lib/me.svelte';
import { router } from '$lib/router.svelte';
import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
import { permissionState } from './fixtures/permission-state.svelte';

vi.mock('$lib/features/feedback/feedback.svelte', async (importOriginal) => ({
  ...(await importOriginal<typeof import('$lib/features/feedback/feedback.svelte')>()),
  notifyDone: vi.fn(),
}));

// 첨부 이름 바꾸기(D-82, 감사 W36): 서버가 이름을 거절하면 입력한 이름이 남고 오류가 칸 옆에 보인다.
// 성공한 뒤에만 입력 칸을 닫는다.
const file: FileView = {
  id: 'f1',
  name: 'report.pdf',
  document_id: null,
  item_id: 'i1',
  current_version: 1,
  size: 1024,
  content_type: 'application/pdf',
  updated_at: '2026-10-21T00:00:00Z',
  can_write: true,
  can_download: true,
};

describe('file rename', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    me.userId = 'user-a';
    localStorage.clear();
    vi.spyOn(api, 'itemFiles').mockResolvedValue([file]);
  });
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    me.userId = '';
    localStorage.clear();
  });

  async function openRename(): Promise<HTMLElement> {
    render(AttachmentsPanel, { props: { target: { kind: 'item', id: 'i1' }, canWrite: true } });
    await fireEvent.click(await screen.findByRole('button', { name: 'report.pdf 이름 바꾸기' }));
    const input = screen.getByRole('textbox', { name: '새 이름' });
    await fireEvent.input(input, { target: { value: 'report/final.pdf' } });
    return input;
  }

  it('keeps a short file name only on screen, guards leaving, and offers nothing after remount', async () => {
    const rename = vi.spyOn(api, 'renameFile');
    await openRename();
    expect(
      Object.keys(localStorage).filter((key) => key.includes('attachment-rename')),
    ).toHaveLength(0);
    const unload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(true);
    cleanup();
    render(AttachmentsPanel, { props: { target: { kind: 'item', id: 'i1' }, canWrite: true } });
    await fireEvent.click(await screen.findByRole('button', { name: 'report.pdf 이름 바꾸기' }));
    expect(screen.queryByRole('button', { name: '입력 되살리기' })).toBeNull();
    expect(screen.getByRole('textbox', { name: '새 이름' })).toHaveValue('report.pdf');
    expect(rename).not.toHaveBeenCalled();
  });

  it('keeps the typed name and shows the error beside it when the server rejects it', async () => {
    const rename = vi
      .spyOn(api, 'renameFile')
      .mockRejectedValue(new ApiError(400, 'bad_request', 'invalid file name'));
    const input = await openRename();
    const form = input.closest('form');
    if (form === null) throw new Error('form');
    await fireEvent.submit(form);

    await vi.waitFor(() => {
      expect(rename).toHaveBeenCalledWith('f1', 'report/final.pdf');
    });
    const kept = await screen.findByRole('textbox', { name: '새 이름' });
    expect(kept).toHaveValue('report/final.pdf');
    await vi.waitFor(() => {
      expect(kept).toHaveAttribute('aria-invalid', 'true');
    });
    const id = kept.getAttribute('aria-describedby');
    expect(id === null ? null : document.getElementById(id)?.textContent.trim()).toBe(
      '처리하지 못했습니다. 입력한 값을 확인해 주세요.',
    );
  });

  it('drops a hidden rename after file permission loss and still guards another unsaved input', async () => {
    const files = permissionState([{ ...file }]);
    vi.spyOn(api, 'itemFiles').mockResolvedValue(files);
    const rename = vi.spyOn(api, 'renameFile');
    await openRename();
    const sibling = render(CreateForm, {
      id: 'other-input',
      label: '다른 입력',
      placeholder: '',
      draftName: 'permission-sibling',
      submitLabel: '만들기',
      create: vi.fn(),
    });
    await fireEvent.input(screen.getByLabelText('다른 입력'), { target: { value: '미저장' } });
    const current = files[0];
    if (current === undefined) throw new Error('missing file');
    current.can_write = false;
    await vi.waitFor(() => {
      expect(screen.queryByRole('textbox', { name: '새 이름' })).toBeNull();
    });
    let unload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(true);
    sibling.unmount();
    unload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(false);
    expect(router.guard?.(vi.fn(), '/')).toBe(true);

    current.name = 'server.pdf';
    current.can_write = true;
    await fireEvent.click(await screen.findByRole('button', { name: 'server.pdf 이름 바꾸기' }));
    expect(screen.getByRole('textbox', { name: '새 이름' })).toHaveValue('server.pdf');
    expect(rename).not.toHaveBeenCalled();
  });

  // 리뷰 지적: 실패 응답이 늦게 오면 그사이 연 다른 파일의 칸에 붙거나(닫았으면) 아무 데도 보이지 않았다.
  function rejectLater(): { reject: (e: unknown) => void } {
    let reject: (e: unknown) => void = () => undefined;
    vi.spyOn(api, 'renameFile').mockImplementation(
      () =>
        new Promise((_, no: (e: unknown) => void) => {
          reject = no;
        }),
    );
    return {
      reject: (e) => {
        reject(e);
      },
    };
  }

  it('does not blame another file opened while the rename was in flight', async () => {
    const other: FileView = { ...file, id: 'f2', name: 'notes.txt', content_type: 'text/plain' };
    vi.spyOn(api, 'itemFiles').mockResolvedValue([file, other]);
    const pending = rejectLater();
    const input = await openRename();
    const form = input.closest('form');
    if (form === null) throw new Error('form');
    await fireEvent.submit(form);
    await fireEvent.click(screen.getByRole('button', { name: 'notes.txt 이름 바꾸기' }));
    await fireEvent.click(await screen.findByRole('button', { name: '변경 버리고 닫기' }));
    const second = screen.getByRole('textbox', { name: '새 이름' });
    expect(second).toHaveValue('notes.txt');

    pending.reject(new ApiError(400, 'bad_request', 'invalid file name'));
    // 응답이 끝나면 저장 단추가 다시 켜진다. 그때 B의 칸은 거절당한 적이 없어야 한다.
    await vi.waitFor(() => {
      expect(screen.getByRole('button', { name: '저장' })).toBeEnabled();
    });
    expect(second.getAttribute('aria-invalid')).not.toBe('true');
    expect(second).not.toHaveAttribute('aria-describedby');
    expect(
      screen.getByText('report.pdf: 처리하지 못했습니다. 입력한 값을 확인해 주세요.'),
    ).toBeTruthy();
  });

  it('still reports the failure after the form was cancelled', async () => {
    const pending = rejectLater();
    const input = await openRename();
    const form = input.closest('form');
    if (form === null) throw new Error('form');
    await fireEvent.submit(form);
    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    await fireEvent.click(await screen.findByRole('button', { name: '변경 버리고 닫기' }));
    expect(screen.queryByRole('textbox', { name: '새 이름' })).toBeNull();

    pending.reject(new ApiError(400, 'bad_request', 'invalid file name'));
    await vi.waitFor(() => {
      expect(
        screen.getByText('report.pdf: 처리하지 못했습니다. 입력한 값을 확인해 주세요.'),
      ).toBeTruthy();
    });
  });

  // 리뷰 지적: 같은 파일의 칸을 닫았다가 다시 열면, 앞 요청의 응답이 새 칸을 닫거나 새 입력에 오류를 붙였다.
  async function submitThenReopen(): Promise<HTMLElement> {
    const input = await openRename();
    const form = input.closest('form');
    if (form === null) throw new Error('form');
    await fireEvent.submit(form);
    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    await fireEvent.click(await screen.findByRole('button', { name: '변경 버리고 닫기' }));
    await fireEvent.click(screen.getByRole('button', { name: 'report.pdf 이름 바꾸기' }));
    const again = screen.getByRole('textbox', { name: '새 이름' });
    await fireEvent.input(again, { target: { value: 'report-v2.pdf' } });
    return again;
  }

  it('keeps a reopened form of the same file when the earlier rename succeeds', async () => {
    let resolve: (renamed: FileView) => void = () => undefined;
    vi.spyOn(api, 'renameFile').mockImplementation(
      () =>
        new Promise((yes: (renamed: FileView) => void) => {
          resolve = yes;
        }),
    );
    await submitThenReopen();

    resolve({ ...file, name: 'report/final.pdf' });
    await vi.waitFor(() => {
      expect(notifyDone).toHaveBeenCalledWith('이름을 바꿨습니다: report/final.pdf');
    });
    expect(screen.getByRole('textbox', { name: '새 이름' })).toHaveValue('report-v2.pdf');
  });

  it('does not mark a reopened form of the same file with the earlier failure', async () => {
    const pending = rejectLater();
    const again = await submitThenReopen();

    pending.reject(new ApiError(400, 'bad_request', 'invalid file name'));
    await vi.waitFor(() => {
      expect(screen.getByRole('button', { name: '저장' })).toBeEnabled();
    });
    expect(again.getAttribute('aria-invalid')).not.toBe('true');
    expect(again).toHaveValue('report-v2.pdf');
    expect(
      screen.getByText('report.pdf: 처리하지 못했습니다. 입력한 값을 확인해 주세요.'),
    ).toBeTruthy();
  });

  it('closes the form once the rename succeeds', async () => {
    vi.spyOn(api, 'renameFile').mockResolvedValue({ ...file, name: 'report/final.pdf' });
    const input = await openRename();
    const form = input.closest('form');
    if (form === null) throw new Error('form');
    await fireEvent.submit(form);
    await vi.waitFor(() => {
      expect(screen.queryByRole('textbox', { name: '새 이름' })).toBeNull();
    });
  });
});
