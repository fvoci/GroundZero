import type { Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api, ApiError } from '$lib/api/client';
import TrashScreen from '$lib/features/trash/TrashScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 보존 기간 입력 오류는 그 칸 옆에 보인다(D-82, 감사 W37). 화면 위 공용 알림이 아니라 칸에 묶인 설명으로.
const workspace: Workspace = {
  access: 'owner',
  id: 'ws-1',
  organization_id: 'org-1',
  name: '연구실',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};

function describedText(input: HTMLElement): string | undefined {
  const id = input.getAttribute('aria-describedby');
  if (id === null) return undefined;
  return document.getElementById(id)?.textContent.trim();
}

describe('trash retention field errors', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'trash').mockResolvedValue({
      retention_days: 30,
      version_retention_days: null,
      entries: [],
    });
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('shows the trash retention range error beside its field', async () => {
    const set = vi.spyOn(api, 'setTrashRetention').mockResolvedValue(undefined);
    render(TrashScreen, { props: { workspace } });
    const input = await screen.findByLabelText('보존 기간(일)');
    await vi.waitFor(() => {
      expect(input).toHaveValue('30');
    });
    await fireEvent.input(input, { target: { value: '0' } });
    const form = input.closest('form');
    if (form === null) throw new Error('form');
    await fireEvent.submit(form);

    expect(set).not.toHaveBeenCalled();
    expect(input).toHaveAttribute('aria-invalid', 'true');
    expect(describedText(input)).toBe('보존 기간은 1~3650일입니다.');
    // 입력은 그대로 남는다.
    expect(input).toHaveValue('0');
  });

  it('shows the version retention range error and a server rejection beside that field', async () => {
    const set = vi
      .spyOn(api, 'setVersionRetention')
      .mockRejectedValueOnce(new ApiError(400, 'bad_request', 'days must be 1..3650'));
    render(TrashScreen, { props: { workspace } });
    const input = await screen.findByLabelText('이전 파일 버전 보존(일, 비우면 영구)');
    const form = input.closest('form');
    if (form === null) throw new Error('form');

    await fireEvent.input(input, { target: { value: '4000' } });
    await fireEvent.submit(form);
    expect(set).not.toHaveBeenCalled();
    expect(input).toHaveAttribute('aria-invalid', 'true');
    expect(describedText(input)).toBe(
      '이전 버전 보존 기간은 1~3650일이거나 비워 둡니다(영구 보존).',
    );

    await fireEvent.input(input, { target: { value: '90' } });
    await fireEvent.submit(form);
    await vi.waitFor(() => {
      expect(describedText(input)).toBe('처리하지 못했습니다. 입력한 값을 확인해 주세요.');
    });
    expect(set).toHaveBeenCalledWith('ws-1', 90);
    expect(input).toHaveValue('90');
  });

  // 리뷰 지적: 같은 칸을 연달아 저장하면, 앞 저장의 늦은 실패가 뒤 저장이 성공한 칸에 오류로 남았다.
  async function saveTwiceThenFailFirst(
    label: string,
    install: (first: () => Promise<undefined>) => void,
  ): Promise<HTMLElement> {
    let reject: (e: unknown) => void = () => undefined;
    install(
      () =>
        new Promise<undefined>((_, no: (e: unknown) => void) => {
          reject = no;
        }),
    );
    render(TrashScreen, { props: { workspace } });
    const days = await screen.findByLabelText('보존 기간(일)');
    await vi.waitFor(() => {
      expect(days).toHaveValue('30');
    });
    const input = screen.getByLabelText(label);
    const form = input.closest('form');
    if (form === null) throw new Error('form');
    await fireEvent.input(input, { target: { value: '40' } });
    await fireEvent.submit(form);
    await fireEvent.input(input, { target: { value: '45' } });
    await fireEvent.submit(form);
    await new Promise((r) => setTimeout(r, 0));

    reject(new ApiError(400, 'bad_request', 'days must be 1..3650'));
    await new Promise((r) => setTimeout(r, 0));
    return input;
  }

  it('does not mark the trash retention field with an earlier save that failed late', async () => {
    const input = await saveTwiceThenFailFirst('보존 기간(일)', (first) => {
      vi.spyOn(api, 'setTrashRetention')
        .mockImplementationOnce(first)
        .mockResolvedValueOnce(undefined);
    });
    expect(input).not.toHaveAttribute('aria-invalid', 'true');
    expect(describedText(input)).toBeUndefined();
    expect(input).toHaveValue('45');
  });

  it('does not mark the version retention field with an earlier save that failed late', async () => {
    const input = await saveTwiceThenFailFirst('이전 파일 버전 보존(일, 비우면 영구)', (first) => {
      vi.spyOn(api, 'setVersionRetention')
        .mockImplementationOnce(first)
        .mockResolvedValueOnce(undefined);
    });
    expect(input).not.toHaveAttribute('aria-invalid', 'true');
    expect(describedText(input)).toBeUndefined();
    expect(input).toHaveValue('45');
  });
});
