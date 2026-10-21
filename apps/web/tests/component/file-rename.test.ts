import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { FileView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import AttachmentsPanel from '$lib/features/files/AttachmentsPanel.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

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
    vi.spyOn(api, 'itemFiles').mockResolvedValue([file]);
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  async function openRename(): Promise<HTMLElement> {
    render(AttachmentsPanel, { props: { target: { kind: 'item', id: 'i1' }, canWrite: true } });
    await fireEvent.click(await screen.findByRole('button', { name: 'report.pdf 이름 바꾸기' }));
    const input = screen.getByRole('textbox', { name: '새 이름' });
    await fireEvent.input(input, { target: { value: 'report/final.pdf' } });
    return input;
  }

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
