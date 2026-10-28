import { homeApi } from '$lib/features/home/api';
import { activityApi } from '$lib/features/activity/api';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import { me } from '$lib/me.svelte';
import { org } from '$lib/org.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { homeView } from '../../stories/fixtures/synthetic';
import { homeExtras } from './fixtures/home';
import { TEST_ORGANIZATION } from './fixtures/organization';
import OrganizationSwitchHost from './fixtures/OrganizationSwitchHost.svelte';

// R5 F-6(⑱·D-121): 워크스페이스 밖 화면에서 조직을 바꿀 때도 이탈 확인을 거친다. 홈 빠른 추가의 초안 이름에는 조직이
// 들어가, 확인 없이 바꾸면 입력이 화면에서 사라진다. 저장소를 쓸 수 없으면(초안을 남기지 못함) 그대로 잃었다.
const OTHER = { ...TEST_ORGANIZATION, id: 'org-2', name: '다른 조직' };

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  // 넓은 화면: 사이드바가 서랍 없이 보인다.
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    addListener: () => undefined,
    removeListener: () => undefined,
    dispatchEvent: () => false,
  }));
  localStorage.clear();
  me.userId = 'user-a';
  org.load('user-a', [TEST_ORGANIZATION, OTHER], TEST_ORGANIZATION.id);
  homeExtras();
  const home = homeView();
  const item = home.active_items.find((value) => value.can_write);
  if (item === undefined) throw new Error('missing writable item');
  localStorage.setItem(org.key('quick-add-item'), item.id);
  vi.spyOn(homeApi, 'home').mockResolvedValue(home);
  vi.spyOn(activityApi, 'notifications').mockResolvedValue({ notifications: [], next: null });
});
afterEach(() => {
  cleanup();
  me.userId = '';
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  localStorage.clear();
});

async function pickOrganization(name: string): Promise<void> {
  await fireEvent.click(screen.getByTestId('organization-switcher'));
  await fireEvent.click(await screen.findByRole('menuitem', { name: new RegExp(name) }));
}

it('asks before switching organizations outside a workspace and keeps unsaved quick-add input', async () => {
  render(OrganizationSwitchHost);
  await fireEvent.click(await screen.findByRole('button', { name: m.home_quick_add() }));
  // 초안을 남기지 못하는 저장소(용량 초과·차단).
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
    throw new DOMException('quota', 'QuotaExceededError');
  });
  await fireEvent.input(await screen.findByLabelText(m.home_title()), {
    target: { value: '잃으면 안 되는 제목' },
  });
  await pickOrganization(OTHER.name);
  await screen.findByRole('alertdialog');
  expect(org.currentId).toBe(TEST_ORGANIZATION.id);
  await fireEvent.click(screen.getByRole('button', { name: m.common_discard_keep() }));
  expect(org.currentId).toBe(TEST_ORGANIZATION.id);
  expect(screen.getByLabelText(m.home_title())).toHaveValue('잃으면 안 되는 제목');

  await pickOrganization(OTHER.name);
  await fireEvent.click(await screen.findByRole('button', { name: m.common_discard_confirm() }));
  await waitFor(() => {
    expect(org.currentId).toBe(OTHER.id);
  });
});
