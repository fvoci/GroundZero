import type { MembersView, OrganizationLimits, Workspace } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import MembersScreen from '$lib/features/members/MembersScreen.svelte';
import { workspaceLimitsApi } from '$lib/features/members/limits-api';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';
import { isRecord } from '$lib/shared/api/parse';

const workspace: Workspace = {
  id: 'ws-1',
  organization_id: 'org-1',
  name: '연구실',
  access: 'write',
  role: 'admin',
  is_owner: false,
};
const view: MembersView = {
  can_manage_members: true,
  members: [],
  my_role: 'admin',
  i_am_owner: false,
  pending_owner: null,
};
const limits: OrganizationLimits = {
  max_file_bytes: 2 * 1024 ** 2,
  max_storage_bytes: null,
  max_concurrent_uploads: 3,
  activity_retention_days: 30,
  audit_retention_days: 60,
};
const orgLimits: OrganizationLimits = {
  max_file_bytes: 100 * 1024 ** 2,
  max_storage_bytes: 4 * 1024 ** 3,
  max_concurrent_uploads: 8,
  activity_retention_days: 90,
  audit_retention_days: 180,
};
const props = { workspace, tab: 'settings' as const, meId: 'u1', onchanged: () => undefined };
let saved: OrganizationLimits;
let puts: unknown[];
let failure: { status: number; body: object } | undefined;

function response(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}
async function panel(): Promise<HTMLElement> {
  const region = await screen.findByRole('region', { name: '한도·보존' });
  await within(region).findByLabelText('활동 보존 일수', { exact: true });
  return region;
}
async function type(label: string, value: string): Promise<HTMLElement> {
  const input = within(await panel()).getByLabelText(label, { exact: true });
  await fireEvent.input(input, { target: { value } });
  return input;
}
async function submit(): Promise<void> {
  await fireEvent.click(within(await panel()).getByRole('button', { name: '저장' }));
}
function description(input: HTMLElement): string {
  return (input.getAttribute('aria-describedby') ?? '')
    .split(' ')
    .map((id) => document.getElementById(id)?.textContent ?? '')
    .join(' ');
}

describe('workspace limits in workspace administration', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    me.userId = 'u1';
    localStorage.clear();
    saved = { ...limits };
    puts = [];
    failure = undefined;
    vi.spyOn(api, 'members').mockResolvedValue(view);
    vi.spyOn(api, 'groups').mockResolvedValue([]);
    vi.spyOn(api, 'invitations').mockResolvedValue([]);
    vi.spyOn(api, 'organization').mockResolvedValue({
      id: 'org-1',
      name: '조직',
      role: 'member',
      public_links_enabled: true,
      created_at: '2026-10-30T00:00:00Z',
      limits: orgLimits,
    });
    vi.spyOn(globalThis, 'fetch').mockImplementation((_input, init) => {
      if (init?.method === 'PUT') {
        if (typeof init.body !== 'string') throw new Error('expected a JSON request body');
        const patch: unknown = JSON.parse(init.body);
        if (!isRecord(patch)) throw new Error('expected a limits object');
        puts.push(patch);
        if (failure !== undefined) return Promise.resolve(response(failure.body, failure.status));
        saved = { ...saved, ...patch };
      }
      return Promise.resolve(response(saved));
    });
  });
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    router.guard = undefined;
    me.userId = '';
    localStorage.clear();
  });

  it.each([
    {
      locale: 'ko',
      heading: '한도·보존',
      field: '활동 보존 일수',
      cap: /^조직 상한:/,
      sizeHint:
        '파일 크기·저장량·동시 업로드 수에 실제로 쓰는 값은 서버 기본값·조직·공간 가운데 가장 작은 값입니다.',
      retentionHint:
        '보존 기간은 조직 값과 공간 값이 모두 있으면 두 값의 최솟값, 한쪽만 있으면 그 값을 씁니다. 둘 다 없으면 서버 설정값(따로 정하지 않았으면 활동 365일·감사 730일)을 씁니다.',
    },
    {
      locale: 'en',
      heading: 'Limits and retention',
      field: 'Activity retention days',
      cap: /^Organization cap:/,
      sizeHint:
        'For file size, storage and concurrent uploads, the effective value is the smallest of the server default, organization and workspace values.',
      retentionHint:
        'For retention periods, use the smaller of the organization and workspace values when both are set, or the set value when only one is set. When neither is set, use the server settings (365 days for activity and 730 days for audit logs unless configured otherwise).',
    },
  ] as const)(
    'shows five fields, organization caps and the separate D-124 explanations in $locale',
    async ({ locale, heading, field, cap, sizeHint, retentionHint }) => {
      await setLocale(locale, { reload: false });
      render(MembersScreen, { props });
      const region = await screen.findByRole('region', { name: heading });
      await within(region).findByLabelText(field, { exact: true });
      expect(within(region).getAllByRole('textbox')).toHaveLength(5);
      expect(within(region).getAllByText(cap)).toHaveLength(5);
      expect(within(region).getByText(sizeHint)).toBeVisible();
      expect(within(region).getByText(retentionHint)).toBeVisible();
      expect(api.organization).toHaveBeenCalledWith('org-1');
      expect(within(region).queryByText(/notification_rate/)).toBeNull();
    },
  );

  it('never requests limits for workspace B using the administrator view loaded for workspace A', async () => {
    const get = vi.spyOn(workspaceLimitsApi, 'get');
    let finish: ((value: MembersView) => void) | undefined;
    const pending = new Promise<MembersView>((resolve) => {
      finish = resolve;
    });
    vi.mocked(api.members).mockResolvedValueOnce(view).mockReturnValueOnce(pending);
    const rendered = render(MembersScreen, { props });
    await panel();
    expect(get).toHaveBeenCalledExactlyOnceWith('ws-1');
    await rendered.rerender({
      ...props,
      workspace: { ...workspace, id: 'ws-2', name: '비관리자 공간 B', role: 'member' },
    });
    expect(api.members).toHaveBeenLastCalledWith('ws-2');
    expect(screen.queryByRole('region', { name: '한도·보존' })).toBeNull();
    expect(get).not.toHaveBeenCalledWith('ws-2');
    if (finish === undefined) throw new Error('workspace B read was not captured');
    finish({ ...view, can_manage_members: false, my_role: 'member' });
    await screen.findByTestId('member-list');
    expect(get).toHaveBeenCalledExactlyOnceWith('ws-1');
    expect(api.organization).toHaveBeenCalledOnce();
    expect(screen.queryByRole('region', { name: '한도·보존' })).toBeNull();
  });

  it.each(['member', 'readonly'] as const)(
    'hides the section from %s even if workspace role says admin',
    async (role) => {
      vi.mocked(api.members).mockResolvedValue({
        ...view,
        can_manage_members: false,
        my_role: role,
      });
      render(MembersScreen, { props });
      await screen.findByTestId('member-list');
      expect(screen.queryByRole('region', { name: '한도·보존' })).toBeNull();
      expect(api.organization).not.toHaveBeenCalled();
      expect(fetch).not.toHaveBeenCalled();
    },
  );

  it('follows can_manage_members for an admin whose display role is member', async () => {
    vi.mocked(api.members).mockResolvedValue({ ...view, my_role: 'member' });
    render(MembersScreen, { props: { ...props, workspace: { ...workspace, role: 'member' } } });
    expect(await panel()).toBeVisible();
  });

  it('sends only changed fields, converts GB to bytes and sends explicit null to clear a field', async () => {
    render(MembersScreen, { props });
    const region = await panel();
    await fireEvent.change(within(region).getByLabelText('공간 전체 저장 공간 설정'), {
      target: { value: 'set' },
    });
    await type('공간 전체 저장 공간', '1.5');
    await fireEvent.change(within(region).getByLabelText('공간 전체 저장 공간 단위'), {
      target: { value: 'GB' },
    });
    await fireEvent.change(within(region).getByLabelText('동시 업로드 수 설정'), {
      target: { value: 'unset' },
    });
    await submit();
    await vi.waitFor(() => {
      expect(puts).toEqual([{ max_storage_bytes: 1.5 * 1024 ** 3, max_concurrent_uploads: null }]);
    });
    expect(within(region).getByLabelText('파일 하나 최대 크기', { exact: true })).toHaveValue('2');
    expect(within(region).getByRole('button', { name: '저장' })).toBeDisabled();
    const init = vi.mocked(fetch).mock.lastCall?.[1];
    expect(new Headers(init?.headers).get('x-fvoci-expected-user')).toBe('u1');
  });

  it.each([
    ['파일 하나 최대 크기', 'max_file_bytes'],
    ['공간 전체 저장 공간', 'max_storage_bytes'],
    ['동시 업로드 수', 'max_concurrent_uploads'],
    ['활동 보존 일수', 'activity_retention_days'],
    ['감사로그 보존 일수', 'audit_retention_days'],
  ] as const)('allows clearing %s to null and explains the disabled input', async (label, key) => {
    saved.max_storage_bytes = 1024 ** 3;
    render(MembersScreen, { props });
    const region = await panel();
    await fireEvent.change(within(region).getByLabelText(`${label} 설정`), {
      target: { value: 'unset' },
    });
    const input = within(region).getByLabelText(label, { exact: true });
    expect(input).toBeDisabled();
    expect(description(input)).toContain('서버 기본값과 조직 값을 따릅니다.');
    await submit();
    await vi.waitFor(() => {
      expect(puts).toEqual([{ [key]: null }]);
    });
  });

  it.each([
    ['파일 하나 최대 크기', '0'],
    ['파일 하나 최대 크기', '1e30'],
    ['동시 업로드 수', '1.5'],
    ['동시 업로드 수', '2147483648'],
    ['활동 보존 일수', '0'],
    ['감사로그 보존 일수', '3651'],
    ['감사로그 보존 일수', ''],
  ])('rejects invalid %s: %s at the field', async (label, value) => {
    render(MembersScreen, { props });
    const input = await type(label, value);
    await submit();
    expect(input).toHaveAttribute('aria-invalid', 'true');
    expect(description(input)).toMatch(/입력하세요/);
    expect(puts).toEqual([]);
  });

  it('accepts zero storage as a configured value and allows unset organization caps', async () => {
    const organization = await api.organization('org-1');
    vi.mocked(api.organization).mockResolvedValue({ ...organization, limits: {} });
    render(MembersScreen, { props });
    const region = await panel();
    expect(within(region).getAllByText('조직 상한: 설정 안 함')).toHaveLength(5);
    await fireEvent.change(within(region).getByLabelText('공간 전체 저장 공간 설정'), {
      target: { value: 'set' },
    });
    await type('공간 전체 저장 공간', '0');
    await type('활동 보존 일수', '3650');
    await submit();
    await vi.waitFor(() => {
      expect(puts).toEqual([{ max_storage_bytes: 0, activity_retention_days: 3650 }]);
    });
  });

  it.each([
    ['활동 보존 일수', '91'],
    ['감사로그 보존 일수', '181'],
  ])('blocks %s over the organization value at its field before sending', async (label, value) => {
    render(MembersScreen, { props });
    const input = await type(label, value);
    expect(description(input)).toContain('조직 값 이하여야 합니다.');
    expect(input).toHaveAttribute('aria-invalid', 'true');
    await submit();
    expect(puts).toEqual([]);
  });

  it.each(['activity_retention_days', 'audit_retention_days'] as const)(
    'maps the server organization cap error to %s and keeps input',
    async (field) => {
      failure = {
        status: 400,
        body: {
          error: 'bad_request',
          field,
          message: `${field} must not exceed the organization value`,
        },
      };
      render(MembersScreen, { props });
      const label = field === 'activity_retention_days' ? '활동 보존 일수' : '감사로그 보존 일수';
      const input = await type(label, '45');
      await submit();
      await vi.waitFor(() => {
        expect(description(input)).toContain('조직 값 이하여야 합니다.');
      });
      expect(input).toHaveValue('45');
      expect(input).toHaveAttribute('aria-invalid', 'true');
    },
  );

  it('maps the unstructured SQL diagnostic to the retention field in English too', async () => {
    await setLocale('en', { reload: false });
    failure = {
      status: 400,
      body: {
        error: 'bad_request',
        message: 'audit_retention_days must not exceed the organization value',
      },
    };
    render(MembersScreen, { props });
    const region = await screen.findByRole('region', { name: 'Limits and retention' });
    const input = await within(region).findByLabelText('Audit log retention days', { exact: true });
    await fireEvent.input(input, { target: { value: '70' } });
    await fireEvent.click(within(region).getByRole('button', { name: 'Save' }));
    await vi.waitFor(() => {
      expect(description(input)).toContain('Must not exceed the organization value.');
    });
    expect(input).toHaveValue('70');
  });

  it('shows skeletons while the workspace settings are still loading', async () => {
    let finish: ((response: Response) => void) | undefined;
    const pending = new Promise<Response>((resolve) => {
      finish = resolve;
    });
    vi.mocked(fetch).mockReturnValueOnce(pending);
    render(MembersScreen, { props });
    const region = await screen.findByRole('region', { name: '한도·보존' });
    const loading = within(region).getByLabelText('불러오는 중…');
    expect(loading).toHaveAttribute('aria-busy', 'true');
    expect(loading.querySelectorAll('[data-slot="skeleton"]')).toHaveLength(5);
    expect(screen.queryByLabelText('활동 보존 일수', { exact: true })).toBeNull();
    if (finish === undefined) throw new Error('load was not captured');
    finish(response(saved));
    expect(await panel()).toBeVisible();
  });

  it('shows an inline read failure and a working retry', async () => {
    vi.mocked(fetch).mockRejectedValueOnce(new TypeError('network unavailable'));
    render(MembersScreen, { props });
    const retry = await screen.findByRole('button', { name: '다시 시도' });
    expect(screen.getByText(/^한도·보존을 불러오지 못했습니다\./)).toBeVisible();
    await fireEvent.click(retry);
    expect(await panel()).toBeVisible();
  });

  it.each([403, 404])('shows the unavailable message and home link on %s', async (status) => {
    vi.mocked(fetch).mockResolvedValueOnce(
      response({ error: status === 403 ? 'forbidden' : 'not_found' }, status),
    );
    render(MembersScreen, { props });
    expect(await screen.findByText('이 조직을 볼 수 없습니다')).toBeVisible();
    expect(screen.getByRole('link', { name: '홈으로 이동' })).toHaveAttribute('href', '/');
    expect(screen.queryByLabelText('활동 보존 일수', { exact: true })).toBeNull();
  });

  it('keeps a conflict inline with input and allows retry', async () => {
    failure = { status: 409, body: { error: 'invalid_state', message: 'settings changed' } };
    render(MembersScreen, { props });
    const input = await type('동시 업로드 수', '5');
    await submit();
    await within(await panel()).findByText(/^한도·보존을 저장하지 못했습니다\./);
    expect(input).toHaveValue('5');
    failure = undefined;
    await submit();
    await vi.waitFor(() => {
      expect(puts).toHaveLength(2);
    });
  });

  it('keeps edits when the same workspace object refreshes and protects them on unload without storing them', async () => {
    const rendered = render(MembersScreen, { props });
    const input = await type('활동 보존 일수', '45');
    await rendered.rerender({ ...props, workspace: { ...workspace, name: '이름 갱신' } });
    expect(input).toHaveValue('45');
    expect(api.organization).toHaveBeenCalledOnce();
    const unload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(true);
    expect(Object.keys(localStorage).filter((key) => key.includes('workspace-limits'))).toEqual([]);
  });

  it('ignores a late save response after another workspace has loaded', async () => {
    let finish: ((response: Response) => void) | undefined;
    const pending = new Promise<Response>((resolve) => {
      finish = resolve;
    });
    const rendered = render(MembersScreen, { props });
    await type('활동 보존 일수', '45');
    vi.mocked(fetch).mockReturnValueOnce(pending);
    await submit();
    await rendered.rerender({ ...props, workspace: { ...workspace, id: 'ws-2' } });
    const next = await within(await panel()).findByLabelText('활동 보존 일수', { exact: true });
    expect(next).toHaveValue('30');
    if (finish === undefined) throw new Error('save was not captured');
    finish(response({ ...limits, activity_retention_days: 45 }));
    await pending;
    await vi.waitFor(() => {
      expect(next).toHaveValue('30');
    });
    expect(within(await panel()).getByRole('button', { name: '저장' })).toBeDisabled();
  });

  it('uses the discard confirmation, preserves on keep and restores saved values on discard', async () => {
    render(MembersScreen, { props });
    const input = await type('활동 보존 일수', '45');
    const leave = vi.fn();
    router.confirmLeave(leave);
    const dialog = await screen.findByRole('alertdialog', {
      name: '저장하지 않은 변경이 있습니다',
    });
    expect(within(dialog).getByText('닫으면 이 창에서 고친 내용이 사라집니다.')).toBeVisible();
    await fireEvent.click(within(dialog).getByRole('button', { name: '계속 편집' }));
    expect(input).toHaveValue('45');
    expect(leave).not.toHaveBeenCalled();
    router.confirmLeave(leave);
    await fireEvent.click(await screen.findByRole('button', { name: '변경 버리고 닫기' }));
    expect(leave).toHaveBeenCalledOnce();
    expect(input).toHaveValue('30');
    expect(puts).toEqual([]);
  });
});
