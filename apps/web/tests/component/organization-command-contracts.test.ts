import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import DisplaySettingsDialog from '$lib/features/settings/DisplaySettingsDialog.svelte';
import { me } from '$lib/me.svelte';
import { org } from '$lib/org.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

import { TEST_ORGANIZATION } from './fixtures/organization';

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'user-a';
  me.locale = 'ko';
  me.weekStart = 'sun';
  me.hourCycle = 'h23';
  org.reset();
});

afterEach(() => {
  org.reset();
  me.userId = '';
  me.locale = '';
  me.weekStart = '';
  me.hourCycle = '';
  vi.unstubAllGlobals();
  localStorage.clear();
});

// #178·D-105·D-96: 조직 선택과 표시 설정은 서로 독립된 명령이다.
it('sends one last-organization command with the selected organization and expected user', async () => {
  const fetch = vi
    .fn<typeof globalThis.fetch>()
    .mockResolvedValue(new Response(null, { status: 204 }));
  vi.stubGlobal('fetch', fetch);
  org.load('user-a', [TEST_ORGANIZATION], null);
  org.select(TEST_ORGANIZATION.id);
  org.select(TEST_ORGANIZATION.id);
  await waitFor(() => {
    expect(fetch).toHaveBeenCalledOnce();
  });
  const call = fetch.mock.calls[0];
  if (call === undefined) throw new Error('missing request');
  const [path, init] = call;
  if (init === undefined || typeof init.body !== 'string') throw new Error('missing JSON body');
  expect(path).toBe('/api/me/last-organization');
  expect(init.method).toBe('PUT');
  expect(JSON.parse(init.body)).toEqual({ organization_id: TEST_ORGANIZATION.id });
  expect(new Headers(init.headers).get('x-fvoci-expected-user')).toBe('user-a');
});

it('saves only display fields when an organization is selected', async () => {
  // 실패 응답으로 대화상자를 유지해 새로 읽기 없이 실제 저장 본문을 검사한다.
  const fetch = vi
    .fn<typeof globalThis.fetch>()
    .mockResolvedValue(
      new Response(JSON.stringify({ error: 'bad_request', message: 'rejected' }), { status: 400 }),
    );
  vi.stubGlobal('fetch', fetch);
  org.load('user-a', [TEST_ORGANIZATION], TEST_ORGANIZATION.id);
  render(DisplaySettingsDialog, { props: { open: true } });
  await fireEvent.click(await screen.findByRole('button', { name: m.common_save() }));
  await screen.findByRole('alert');
  expect(fetch).toHaveBeenCalledOnce();
  const call = fetch.mock.calls[0];
  if (call === undefined) throw new Error('missing request');
  const [path, init] = call;
  if (init === undefined || typeof init.body !== 'string') throw new Error('missing JSON body');
  expect(path).toBe('/api/me/settings');
  expect(init.method).toBe('PUT');
  expect(JSON.parse(init.body)).toEqual({
    time_zone: me.timeZone,
    locale: 'ko',
    week_start: 'sun',
    hour_cycle: 'h23',
    theme: me.theme,
  });
  expect(new Headers(init.headers).get('x-fvoci-expected-user')).toBe('user-a');
});
