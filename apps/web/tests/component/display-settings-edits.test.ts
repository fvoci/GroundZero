import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import { conventionalHourCycle, conventionalWeekStart } from '$lib/display';
import { applyDocFont, storedDocFont } from '$lib/doc-font';
import DisplaySettingsDialog from '$lib/features/settings/DisplaySettingsDialog.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 칸 이름. 시각 표시·서체 칸의 label에는 예시·안내 글도 들어 있어 앞부분으로 찾는다.
const LANGUAGE = /^언어/;
const ZONE = /^시간대/;
const WEEK = /^한 주 시작/;
const CLOCK = /^시각 표시/;
const FONT = /^문서 본문 서체/;

beforeEach(async () => {
  await setLocale('ko', { reload: false });
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  // 저장이 성공하면 대화상자가 me를 고친다. 다음 시험이 앞 시험의 저장 값을 보지 않게 비운다.
  Object.assign(me, { timeZone: '', locale: '', weekStart: '', hourCycle: '' });
  window.localStorage.clear();
  applyDocFont('sans');
});

/** 저장 뒤 화면을 새로 읽는 대신 부른 횟수만 센다(jsdom의 location.reload는 바꿔 끼울 수 없다). */
function stubReload(): ReturnType<typeof vi.fn> {
  const reload = vi.fn();
  vi.stubGlobal('location', { reload });
  return reload;
}

async function choose(label: RegExp, value: string): Promise<void> {
  await fireEvent.change(await screen.findByLabelText(label), { target: { value } });
}

/** 저장을 누르고 계정 저장 뒤의 새로 읽기까지 기다린다. */
async function saveAndReload(): Promise<void> {
  const reload = stubReload();
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  await waitFor(() => {
    expect(reload).toHaveBeenCalledTimes(1);
  });
}

// 감사(실제 브라우저): 열 때 칸을 채우는 효과가 자기가 쓴 칸을 의존으로 잡아, 고를 때마다 저장된 값으로 되돌아갔다.
// 그래서 무엇을 골라도 저장하면 예전 값이 그대로 갔다(D-50, D-81).
describe('display settings dialog keeps edits until save', () => {
  it('saves every changed field when settings were saved before', async () => {
    Object.assign(me, { timeZone: 'Asia/Seoul', locale: 'ko', weekStart: 'sun', hourCycle: 'h12' });
    const set = vi.spyOn(api, 'setDisplaySettings').mockResolvedValue(undefined);
    render(DisplaySettingsDialog, { props: { open: true } });

    // 저장한 주 시작·시각 표시는 언어를 바꿔도 다시 제안하지 않는다(언어를 나중에 바꾼다). 두 언어의 관례와 다른
    // 값(월요일·24시간)을 골라야 다시 제안했는지 드러난다.
    await choose(WEEK, 'mon');
    await choose(CLOCK, 'h23');
    await choose(LANGUAGE, 'en');
    await choose(ZONE, 'America/New_York');
    await saveAndReload();

    expect(set).toHaveBeenCalledTimes(1);
    expect(set).toHaveBeenCalledWith({
      time_zone: 'America/New_York',
      locale: 'en',
      week_start: 'mon',
      hour_cycle: 'h23',
      theme: 'system',
    });
  });

  it('saves every changed field when nothing was saved yet', async () => {
    const set = vi.spyOn(api, 'setDisplaySettings').mockResolvedValue(undefined);
    render(DisplaySettingsDialog, { props: { open: true } });

    // 언어를 먼저 바꾼다(아직 저장하지 않은 칸은 언어를 바꾸면 다시 제안하므로).
    await choose(LANGUAGE, 'ko');
    await choose(ZONE, 'Asia/Seoul');
    await choose(WEEK, 'mon');
    await choose(CLOCK, 'h23');
    await saveAndReload();

    expect(set).toHaveBeenCalledTimes(1);
    expect(set).toHaveBeenCalledWith({
      time_zone: 'Asia/Seoul',
      locale: 'ko',
      week_start: 'mon',
      hour_cycle: 'h23',
      theme: 'system',
    });
  });

  it('re-suggests unsaved week start and clock from the newly chosen language', async () => {
    render(DisplaySettingsDialog, { props: { open: true } });
    // 관례와 다른 값을 먼저 고른다(같으면 다시 제안했는지 알 수 없다).
    await choose(WEEK, 'mon');
    await choose(CLOCK, 'h23');
    await choose(LANGUAGE, 'ko');
    expect(conventionalHourCycle('ko')).not.toBe('h23');
    expect(screen.getByLabelText(LANGUAGE)).toHaveValue('ko');
    expect(screen.getByLabelText(WEEK)).toHaveValue(conventionalWeekStart('ko'));
    expect(screen.getByLabelText(CLOCK)).toHaveValue(conventionalHourCycle('ko'));
  });

  it('starts again from the saved values each time it opens', async () => {
    Object.assign(me, { timeZone: 'Asia/Seoul', locale: 'ko', weekStart: 'sun', hourCycle: 'h23' });
    const view = render(DisplaySettingsDialog, { props: { open: true } });
    await choose(LANGUAGE, 'en');
    await choose(ZONE, 'America/New_York');
    await choose(WEEK, 'mon');
    await choose(CLOCK, 'h12');
    await fireEvent.click(screen.getByRole('button', { name: '취소' }));

    // 저장하지 않고 닫았다 다시 열면 고르던 값을 버리고 저장한 값에서 시작한다.
    await view.rerender({ open: false });
    await view.rerender({ open: true });
    expect(await screen.findByLabelText(LANGUAGE)).toHaveValue('ko');
    expect(screen.getByLabelText(ZONE)).toHaveValue('Asia/Seoul');
    expect(screen.getByLabelText(WEEK)).toHaveValue('sun');
    expect(screen.getByLabelText(CLOCK)).toHaveValue('h23');

    // 그사이 저장된 값이 바뀌었으면(다른 탭에서 저장한 뒤 새로 읽음) 다시 열 때 그 값을 쓴다.
    await view.rerender({ open: false });
    Object.assign(me, {
      timeZone: 'Europe/Paris',
      locale: 'en',
      weekStart: 'mon',
      hourCycle: 'h12',
    });
    await view.rerender({ open: true });
    expect(await screen.findByLabelText(LANGUAGE)).toHaveValue('en');
    expect(screen.getByLabelText(ZONE)).toHaveValue('Europe/Paris');
    expect(screen.getByLabelText(WEEK)).toHaveValue('mon');
    expect(screen.getByLabelText(CLOCK)).toHaveValue('h12');
  });

  it('does not write the account settings when only the document typeface changed', async () => {
    Object.assign(me, { timeZone: 'Asia/Seoul', locale: 'ko', weekStart: 'sun', hourCycle: 'h23' });
    const set = vi.spyOn(api, 'setDisplaySettings').mockResolvedValue(undefined);
    render(DisplaySettingsDialog, { props: { open: true } });
    await choose(FONT, 'serif');

    const reload = stubReload();
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(storedDocFont()).toBe('serif');
    });
    expect(set).not.toHaveBeenCalled();
    expect(reload).not.toHaveBeenCalled();
  });

  it('still saves the account settings when the typeface changed together with them', async () => {
    Object.assign(me, { timeZone: 'Asia/Seoul', locale: 'ko', weekStart: 'sun', hourCycle: 'h23' });
    const set = vi.spyOn(api, 'setDisplaySettings').mockResolvedValue(undefined);
    render(DisplaySettingsDialog, { props: { open: true } });
    await choose(FONT, 'serif');
    await choose(WEEK, 'mon');
    await saveAndReload();

    expect(storedDocFont()).toBe('serif');
    expect(set).toHaveBeenCalledWith({
      time_zone: 'Asia/Seoul',
      locale: 'ko',
      week_start: 'mon',
      hour_cycle: 'h23',
      theme: 'system',
    });
  });
});
