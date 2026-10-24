import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { api, ApiError } from '$lib/api/client';
import { applyDocFont, setDocFont } from '$lib/doc-font';
import DisplaySettingsDialog from '$lib/features/settings/DisplaySettingsDialog.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { applyTheme, THEME_KEY } from '$lib/theme.svelte';

afterEach(() => {
  vi.restoreAllMocks();
  applyDocFont('sans');
  applyTheme('light');
  window.localStorage.clear();
  me.locale = '';
  me.timeZone = '';
  me.weekStart = '';
  me.hourCycle = '';
  me.theme = 'system';
  vi.unstubAllGlobals();
});

async function openDialog(): Promise<HTMLSelectElement[]> {
  await setLocale('ko', { reload: false });
  me.locale = 'ko';
  me.weekStart = 'sun';
  me.hourCycle = 'h23';
  render(DisplaySettingsDialog, { props: { open: true } });
  await screen.findByText(m.settings_title());
  return [...document.querySelectorAll('select')];
}

// 감사 W26: 표시 설정 대화상자.
describe('display settings dialog', () => {
  // (c) 서버 오류(세션 만료 401 등)를 영어 원문 그대로("Unauthorized") 보였다. 다른 화면처럼 errorText로 옮긴다.
  it('shows a failed save in the display language', async () => {
    me.timeZone = 'Asia/Seoul';
    vi.spyOn(api, 'setDisplaySettings').mockRejectedValue(
      new ApiError(401, 'unauthorized', 'Unauthorized'),
    );
    await openDialog();
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).not.toBe('Unauthorized');
    expect(alert.textContent).toContain(m.settings_save_failed());
    expect(alert.textContent).toContain(m.common_error_unauthorized());
  });

  // (b) 다른 브라우저(엔진)에서 저장한 시간대가 이 브라우저 목록에 없으면 고른 칸이 비어 보였다.
  it('shows a saved zone this browser does not list', async () => {
    me.timeZone = 'Asia/Kolkata';
    vi.spyOn(Intl, 'supportedValuesOf').mockReturnValue(['Asia/Calcutta', 'Asia/Seoul']);
    const [, zone] = await openDialog();
    if (zone === undefined) throw new Error('no zone select');
    expect(zone.value).toBe('Asia/Kolkata');
  });

  // (a) 저장소를 쓸 수 없는 브라우저: 명조는 이 화면에만 적용돼 있는데 대화상자는 저장소를 읽어 고딕으로 보였고, 다른
  // 칸을 바꿔 저장하면 고딕으로 되돌렸다.
  it('shows and keeps the applied font when it could not be stored', async () => {
    const blocked = (): never => {
      throw new DOMException('blocked', 'SecurityError');
    };
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(blocked);
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(blocked);
    vi.spyOn(Storage.prototype, 'removeItem').mockImplementation(blocked);
    setDocFont('serif');
    expect(document.documentElement.dataset.docFont).toBe('serif');

    me.timeZone = 'Asia/Seoul';
    vi.spyOn(api, 'setDisplaySettings').mockRejectedValue(
      new ApiError(401, 'unauthorized', 'Unauthorized'),
    );
    const selects = await openDialog();
    const [, , week, , font] = selects;
    if (week === undefined || font === undefined) throw new Error('missing selects');
    expect(font.value).toBe('serif');

    await fireEvent.change(week, { target: { value: 'mon' } });
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    await screen.findByRole('alert');
    expect(document.documentElement.dataset.docFont).toBe('serif');
  });
});

// 화면 밝기(시스템 설정 따르기·밝게·어둡게, D-109)는 다른 표시 설정처럼 계정에 둔다. 이 브라우저에는 첫 그림용 사본만 둔다.
// 이것만 바꾸면 바로 적용하고 화면을 새로 읽지 않는다.
describe('display settings dialog theme', () => {
  it('offers system, light and dark, saves the choice to the account and applies it without a reload', async () => {
    const saveAccount = vi.spyOn(api, 'setDisplaySettings').mockResolvedValue(undefined);
    const reload = vi.fn();
    vi.stubGlobal('location', { reload });
    me.theme = 'system';
    applyTheme('system');
    await openDialog();
    const theme = screen.getByRole('combobox', { name: /^화면 밝기/ });
    expect(theme).toHaveValue('system');
    expect([...theme.querySelectorAll('option')].map((o) => o.textContent)).toEqual([
      '시스템 설정 따르기',
      '밝게',
      '어둡게',
    ]);

    await fireEvent.change(theme, { target: { value: 'dark' } });
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    await vi.waitFor(() => {
      expect(saveAccount).toHaveBeenCalledWith(expect.objectContaining({ theme: 'dark' }));
    });
    await vi.waitFor(() => {
      expect(document.documentElement).toHaveClass('dark');
    });
    expect(me.theme).toBe('dark');
    // 다음 첫 그림(index.html)을 위한 사본.
    expect(window.localStorage.getItem(THEME_KEY)).toBe('dark');
    expect(reload).not.toHaveBeenCalled();
  });
});
