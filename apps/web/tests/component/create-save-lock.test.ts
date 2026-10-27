import type { Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import { formDraftTabKey, writeFormDraftAt } from '$lib/features/common/form-draft';
import RecurrenceEditor from '$lib/features/events/RecurrenceEditor.svelte';
import HomeDashboard from '$lib/features/home/HomeDashboard.svelte';
import ModulesSection from '$lib/features/modules/ModulesSection.svelte';
import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
import { me } from '$lib/me.svelte';
import { org } from '$lib/org.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

import { homeView } from '../../stories/fixtures/synthetic';
import { homeExtras } from './fixtures/home';

// R5 2차 확인 필요 2: F-16과 같은 부류를 FormDraftOffer 사용처에서 찾았다. 저장 중에 입력이나 다른 탭 초안의
// 되살리기가 열려 있으면 스냅숏이 바뀌어 `complete()`가 거짓이 되고, 만들기는 성공을 버린 채 다시 내면 새 요청 id로
// 하나 더 만들고(빠른 업무·모듈·공용 만들기 폼), 반복 규칙은 기준 규칙을 옮기지 못해 다음 저장이 자기 저장과 부딪친다.

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  localStorage.clear();
  me.userId = 'user-a';
  Element.prototype.hasPointerCapture = (): boolean => false;
  Element.prototype.setPointerCapture = (): void => undefined;
  Element.prototype.releasePointerCapture = (): void => undefined;
  Element.prototype.scrollIntoView = (): void => undefined;
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  me.userId = '';
});

/** 같은 사용자의 다른 탭이 남긴 초안(이 탭에는 되살리기 제안으로 보인다). */
function otherTabDraft(name: string, input: unknown): void {
  const own = formDraftTabKey(name, 'user-a');
  if (own === undefined) throw new Error('draft key');
  writeFormDraftAt(own.replace(/:tab:.*$/, ':tab:other-tab'), {
    savedAt: '2026-10-27T00:00:00.000Z',
    input,
  });
}

function expectOfferLocked(): void {
  expect(screen.getByRole('button', { name: m.common_draft_restore() })).toBeDisabled();
  expect(screen.getByRole('button', { name: m.common_draft_discard() })).toBeDisabled();
}

/** bits-ui Select는 pointerdown에서 연다. 열렸는지는 트리거의 aria-expanded로 본다. */
async function opens(labelledBy: string): Promise<boolean> {
  const el = document.querySelector<HTMLElement>(`[aria-labelledby="${labelledBy}"]`);
  if (el === null) throw new Error(labelledBy);
  await fireEvent.pointerDown(el, { button: 0, pointerType: 'mouse' });
  await new Promise((r) => setTimeout(r, 30));
  return el.getAttribute('aria-expanded') === 'true';
}

const workspace: Workspace = {
  id: 'ws',
  organization_id: 'org-1',
  name: '공간',
  access: 'owner',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};

describe('create forms while the request is in flight', () => {
  it('locks the home quick-add draft offer', async () => {
    homeExtras();
    const home = homeView();
    const item = home.active_items.find((value) => value.can_write);
    if (item === undefined) throw new Error('missing writable item');
    localStorage.setItem(org.key('quick-add-item'), item.id);
    otherTabDraft('home-quick-task:org-1', { title: '다른 탭 업무', item: item.id, pending: null });
    vi.spyOn(api, 'home').mockResolvedValue(home);
    vi.spyOn(api, 'notifications').mockResolvedValue({ notifications: [], next: null });
    const add = vi.spyOn(api, 'createQuickTask').mockReturnValue(new Promise(() => undefined));
    render(HomeDashboard, { workspaces: [] });
    await fireEvent.click(await screen.findByRole('button', { name: m.home_quick_add() }));
    await fireEvent.input(await screen.findByLabelText(m.home_title()), {
      target: { value: '이 탭 업무' },
    });
    await fireEvent.click(screen.getByRole('button', { name: m.home_add() }));
    await waitFor(() => {
      expect(add).toHaveBeenCalledOnce();
    });
    expectOfferLocked();
  });

  it('locks the new module form and its draft offer', async () => {
    vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
    otherTabDraft(`module-new:${workspace.id}`, {
      name: '다른 탭 모듈',
      key: 'OTH',
      kind: 'project',
      pending: null,
    });
    const create = vi.spyOn(api, 'createModule').mockReturnValue(new Promise(() => undefined));
    render(ModulesSection, { workspace });
    const name = screen.getByLabelText(m.modules_name());
    const key = screen.getByLabelText(m.modules_key());
    await fireEvent.input(name, { target: { value: '새 모듈' } });
    await fireEvent.input(key, { target: { value: 'NEW' } });
    await fireEvent.click(screen.getByRole('button', { name: m.modules_create() }));
    await waitFor(() => {
      expect(create).toHaveBeenCalledOnce();
    });
    expect(name).toBeDisabled();
    expect(key).toBeDisabled();
    expect(await opens('module-kind-label')).toBe(false);
    expectOfferLocked();
  });

  it('locks the shared create form draft offer', async () => {
    otherTabDraft('test:lock:container', { name: '다른 탭 이름', requestId: null, context: '' });
    const create = vi.fn(() => new Promise<void>(() => undefined));
    render(CreateForm, {
      id: 'new',
      label: 'Name',
      placeholder: '',
      submitLabel: 'Create',
      draftName: 'test:lock:container',
      create,
    });
    await fireEvent.input(screen.getByRole('textbox', { name: 'Name' }), {
      target: { value: '이 탭 이름' },
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Create' }));
    await waitFor(() => {
      expect(create).toHaveBeenCalledOnce();
    });
    expectOfferLocked();
  });
});

describe('recurrence editor while a save is in flight', () => {
  it('locks the rule inputs, frequency and draft offer', async () => {
    const fields = {
      freq: 'daily',
      interval: '3',
      weekdays: [],
      end: 'never',
      until: '',
      count: '',
    };
    otherTabDraft('rule-lock', { fields, baseFields: { ...fields, interval: '1' }, base: null });
    const onsave = vi.fn(() => new Promise<void>(() => undefined));
    render(RecurrenceEditor, {
      idPrefix: 'rule',
      rule: { freq: 'daily', interval: 1, weekdays: [], until: null, count: null },
      canEdit: true,
      zoneNote: 'UTC',
      onsave,
      draftName: 'rule-lock',
    });
    const interval = screen.getByLabelText(m.events_interval());
    await fireEvent.input(interval, { target: { value: '2' } });
    await fireEvent.click(screen.getByRole('button', { name: m.events_recurrence_save() }));
    await waitFor(() => {
      expect(onsave).toHaveBeenCalledOnce();
    });
    expect(interval).toBeDisabled();
    expect(await opens('rule-freq-label')).toBe(false);
    expectOfferLocked();
  });
});
