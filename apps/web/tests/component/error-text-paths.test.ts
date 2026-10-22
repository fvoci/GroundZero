import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api, ApiError } from '$lib/api/client';
import { errorText } from '$lib/api/errors';
import { me } from '$lib/me.svelte';
import DisplaySettingsDialog from '$lib/features/settings/DisplaySettingsDialog.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 번역되지 않거나 이유가 빠지던 오류 문구(D-81·D-82, 감사 W33).
describe('error text paths', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('display settings show the translated reason instead of the server English', async () => {
    me.locale = 'ko';
    me.weekStart = 'mon';
    me.hourCycle = 'h23';
    const save = vi
      .spyOn(api, 'setDisplaySettings')
      .mockRejectedValue(new ApiError(401, 'unauthorized', 'Unauthorized'));
    render(DisplaySettingsDialog, { props: { open: true } });
    await fireEvent.click(await screen.findByRole('button', { name: '저장' }));
    await vi.waitFor(() => {
      expect(save).toHaveBeenCalled();
    });
    const alert = await screen.findByRole('alert');
    expect(alert).not.toHaveTextContent('Unauthorized');
    expect(alert).toHaveTextContent('저장하지 못했습니다. 다시 로그인해 주세요.');
  });

  it('gives a reason for a body that is too large', () => {
    expect(
      errorText(new ApiError(413, 'too_large', 'payload too large'), '저장하지 못했습니다.'),
    ).toBe('저장하지 못했습니다. 보내는 내용이 너무 큽니다. 줄인 뒤 다시 시도해 주세요.');
  });

  it('keeps the translated status of an unexpected (non-JSON) response', () => {
    // client.ts가 앞단 프록시의 HTML 응답 같은 것을 번역된 문구와 상태로 바꿔 둔다.
    const unexpected = new ApiError(413, 'unexpected', '예상하지 못한 응답을 받았습니다(413).');
    expect(errorText(unexpected, '저장하지 못했습니다.')).toBe(
      '저장하지 못했습니다. 예상하지 못한 응답을 받았습니다(413).',
    );
  });
});
