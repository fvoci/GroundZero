import { fireEvent, screen, within } from '@testing-library/svelte';

import { m } from '$lib/paraglide/messages.js';
import { router } from '$lib/router.svelte';

// 항목 화면은 고른 묶음만 마운트한다(D-121). 묶음 안 패널을 보는 시험은 먼저 그 묶음으로 간다.
export async function openItemGroup(label: string): Promise<void> {
  const nav = await screen.findByRole('navigation', { name: m.modules_item_groups_label() });
  await fireEvent.click(within(nav).getByRole('link', { name: label }));
}

// 모듈 화면의 달력은 달력 보기에서만 마운트한다(D-121).
export async function openModuleView(label: string): Promise<void> {
  const nav = await screen.findByRole('navigation', { name: m.modules_views_label() });
  await fireEvent.click(within(nav).getByRole('link', { name: label }));
}

/** 묶음 전환이 바꾼 주소를 다음 시험에 남기지 않는다. */
export function resetRoute(): void {
  window.history.replaceState(null, '', '/');
  router.path = '/';
  router.hash = '';
}
