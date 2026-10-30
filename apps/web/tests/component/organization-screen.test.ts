import { fireEvent, render, screen, within, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import { ApiError } from '$lib/shared/api';
import { organizationsApi } from '$lib/features/organizations/api';
import OrganizationScreen from '$lib/features/organizations/OrganizationScreen.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { organizationPath, parseRoute, router } from '$lib/router.svelte';

const id = '00000000-0000-0000-0000-000000000001';
const organization = {
  id,
  name: '시험 조직',
  role: 'admin' as const,
  public_links_enabled: true,
  limits: {},
  created_at: '2026-10-01T00:00:00Z',
};
const members = [
  {
    user_id: 'user-a',
    display_name: '첫 관리자',
    email: 'admin@example.test',
    role: 'admin' as const,
    created_at: organization.created_at,
  },
];
const firstMember = members[0];
if (firstMember === undefined) throw new Error('missing administrator fixture');

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'user-a';
  vi.spyOn(api, 'organization').mockResolvedValue(organization);
  vi.spyOn(organizationsApi, 'members').mockResolvedValue(members);
  router.replace(organizationPath(id));
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  me.userId = '';
  router.guard = undefined;
});

function view(tab: 'members' | 'invitations' | 'workspaces' | 'settings' = 'members') {
  router.replace(organizationPath(id, tab));
  return render(OrganizationScreen, {
    organizationId: id,
    tab,
    meId: 'user-a',
    onchanged: () => undefined,
  });
}

it('disables demotion and removal of the last administrator and explains why', async () => {
  view();
  await fireEvent.click(await screen.findByRole('button', { name: '첫 관리자 동작' }));
  for (const name of ['멤버로 바꾸기', '내보내기']) {
    const action = screen.getByRole('menuitem', { name });
    expect(action).toHaveAttribute('aria-disabled', 'true');
    expect(action).toHaveAccessibleDescription('마지막 관리자는 바꿀 수 없습니다');
  }
});

it('confirms self demotion with the loss of management and joined roles', async () => {
  vi.mocked(organizationsApi.members).mockResolvedValue([
    ...members,
    { ...firstMember, user_id: 'other', display_name: '둘째 관리자' },
  ]);
  view();
  await fireEvent.click(await screen.findByRole('button', { name: '첫 관리자 동작' }));
  await fireEvent.click(screen.getByRole('menuitem', { name: '멤버로 바꾸기' }));
  const dialog = await screen.findByRole('dialog');
  expect(dialog).toHaveTextContent(
    "관리 탭을 더 볼 수 없습니다. '관리자로 참여'로 받은 공간 역할은 참여 전으로 돌아갑니다(D-112).",
  );
  expect(within(dialog).getByRole('button', { name: '멤버로 바꾸기' })).toBeEnabled();
});

it.each(['취소', '닫기'])(
  'does not carry a failed removal preview into self demotion after %s',
  async (closeLabel) => {
    vi.mocked(organizationsApi.members).mockResolvedValue([
      ...members,
      { ...firstMember, user_id: 'other', display_name: '둘째 관리자' },
    ]);
    const preview = vi
      .spyOn(organizationsApi, 'removalPreview')
      .mockRejectedValue(new ApiError(500, 'internal', '제거 미리보기를 불러오지 못했습니다'));
    view();
    await fireEvent.click(await screen.findByRole('button', { name: '둘째 관리자 동작' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: '내보내기' }));
    const removal = await screen.findByRole('dialog');
    await within(removal).findByText('제거 미리보기를 불러오지 못했습니다');
    expect(within(removal).getByRole('button', { name: '다시 시도' })).toBeEnabled();
    await fireEvent.click(within(removal).getByRole('button', { name: closeLabel }));

    await fireEvent.click(screen.getByRole('button', { name: '첫 관리자 동작' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: '멤버로 바꾸기' }));
    const demotion = await screen.findByRole('dialog', { name: '내 역할을 멤버로 바꿀까요?' });
    expect(demotion).not.toHaveTextContent('제거 미리보기를 불러오지 못했습니다');
    expect(within(demotion).queryByRole('button', { name: '다시 시도' })).toBeNull();
    expect(within(demotion).getByRole('button', { name: '멤버로 바꾸기' })).toBeEnabled();
    expect(preview).toHaveBeenCalledExactlyOnceWith(id, 'other');
  },
);

it.each([
  [
    { workspaces: 2, shares: 0, assignments: 3 },
    '대상님을 조직에서 내보내면 공간 2곳에서 빠지고, 담당 3건이 풀립니다.',
  ],
  [{ workspaces: 0, shares: 0, assignments: 0 }, '영향받는 공간·공유·담당이 없습니다'],
  [
    { workspaces: 1, shares: 0, assignments: 0 },
    '대상님을 조직에서 내보내면 공간 1곳에서 빠집니다.',
  ],
  [
    { workspaces: 0, shares: 2, assignments: 0 },
    '대상님을 조직에서 내보내면 공유 2개가 거둬집니다.',
  ],
])(
  'omits zero counts from removal preview and retains a server conflict in the dialog',
  async (preview, text) => {
    vi.mocked(organizationsApi.members).mockResolvedValue([
      ...members,
      { ...firstMember, user_id: 'target', display_name: '대상', role: 'member' },
    ]);
    const fetch = vi.fn<typeof globalThis.fetch>().mockImplementation((_path, init) =>
      Promise.resolve(
        init?.method === 'DELETE'
          ? new Response(
              JSON.stringify({
                error: 'invalid_state',
                message: '진행 중인 편집을 마친 뒤 다시 시도하세요',
              }),
              { status: 409 },
            )
          : new Response(JSON.stringify(preview)),
      ),
    );
    vi.stubGlobal('fetch', fetch);
    view();
    await fireEvent.click(await screen.findByRole('button', { name: '대상 동작' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: '내보내기' }));
    const dialog = await screen.findByRole('dialog');
    await within(dialog).findByText(text);
    expect(dialog).not.toHaveTextContent('공유 0');
    await fireEvent.click(within(dialog).getByRole('button', { name: '내보내기' }));
    await waitFor(() =>
      expect(within(dialog).getByRole('alert')).toHaveTextContent(
        '진행 중인 편집을 마친 뒤 다시 시도하세요',
      ),
    );
    expect(within(dialog).getByRole('button', { name: '내보내기' })).toBeEnabled();
  },
);

it('keeps invitation input on conflict and maps email errors to the input', async () => {
  const fetch = vi
    .fn<typeof globalThis.fetch>()
    .mockImplementation((_path, init) =>
      Promise.resolve(
        init?.method === 'POST'
          ? new Response(
              JSON.stringify({ error: 'invalid_state', message: '이미 초대를 보냈습니다' }),
              { status: 409 },
            )
          : new Response('[]'),
      ),
    );
  vi.stubGlobal('fetch', fetch);
  view('invitations');
  const email = await screen.findByRole('textbox', { name: '이메일' });
  await fireEvent.input(email, { target: { value: 'invite@example.test' } });
  await fireEvent.click(screen.getByRole('button', { name: '초대 보내기' }));
  await screen.findByText('이미 초대를 보냈습니다');
  expect(email).toHaveValue('invite@example.test');
  fetch.mockResolvedValue(
    new Response(
      JSON.stringify({
        error: 'bad_request',
        message: 'invalid email',
        field: 'email',
        reason: 'invalid_format',
      }),
      { status: 400 },
    ),
  );
  await fireEvent.click(screen.getByRole('button', { name: '초대 보내기' }));
  await waitFor(() => expect(email).toHaveAttribute('aria-invalid', 'true'));
  expect(email).toHaveAccessibleDescription(/이메일/);
});

it('requires all three disclosures before joining as administrator', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn<typeof globalThis.fetch>().mockResolvedValue(
      new Response(
        JSON.stringify([
          {
            id: 'ws-1',
            name: '연구 공간',
            owner_id: 'other',
            owner_name: '소유자',
            member_count: 2,
            created_at: organization.created_at,
            my_role: null,
          },
        ]),
      ),
    ),
  );
  view('workspaces');
  await fireEvent.click(await screen.findByRole('button', { name: '관리자로 참여' }));
  const dialog = await screen.findByRole('dialog', {
    name: '연구 공간 공간에 관리자로 참여할까요?',
  });
  expect(dialog).toHaveTextContent('이 공간의 모든 문서와 기록을 볼 수 있게 됩니다.');
  expect(dialog).toHaveTextContent('참여한 사실은 감사로그에 남습니다.');
  expect(dialog).toHaveTextContent('조직 관리자에서 내려가면 이 역할은 참여 전으로 돌아갑니다.');
});

it('confirms disabling public links and saves only changed fields including explicit null', async () => {
  vi.mocked(api.organization).mockResolvedValue({
    ...organization,
    limits: { max_concurrent_uploads: 3 },
  });
  const fetch = vi
    .fn<typeof globalThis.fetch>()
    .mockResolvedValue(
      new Response(
        JSON.stringify({ ...organization, name: '새 이름', public_links_enabled: false }),
      ),
    );
  vi.stubGlobal('fetch', fetch);
  view('settings');
  await fireEvent.input(await screen.findByRole('textbox', { name: '이름' }), {
    target: { value: '새 이름' },
  });
  await fireEvent.click(screen.getByRole('switch', { name: '공개 링크 허용' }));
  const dialog = await screen.findByRole('dialog');
  expect(dialog).toHaveTextContent(
    '끄면 이 조직의 모든 공개 링크가 바로 열리지 않습니다. 다시 켜면 만료·철회되지 않은 링크는 다시 열립니다.',
  );
  await fireEvent.click(within(dialog).getByRole('button', { name: '공개 링크 끄기' }));
  await fireEvent.click(screen.getByRole('checkbox', { name: '동시 업로드 수 설정 안 함' }));
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  await waitFor(() => {
    expect(fetch).toHaveBeenCalledOnce();
  });
  expect(fetch.mock.calls[0]?.[0]).toBe(`/api/organizations/${id}/settings`);
  const body = fetch.mock.calls[0]?.[1]?.body;
  if (typeof body !== 'string') throw new Error('missing settings request body');
  expect(JSON.parse(body)).toEqual({
    name: '새 이름',
    public_links_enabled: false,
    max_concurrent_uploads: null,
  });
});

it('keeps public links enabled when the disable confirmation is cancelled', async () => {
  view('settings');
  const links = await screen.findByRole('switch', { name: '공개 링크 허용' });
  expect(links).toBeChecked();
  await fireEvent.click(links);
  const dialog = await screen.findByRole('dialog');
  await fireEvent.click(within(dialog).getByRole('button', { name: '취소' }));
  expect(links).toBeChecked();
  expect(screen.getByRole('button', { name: '저장' })).toBeDisabled();
});

it('shows unavailable and a home link on 403', async () => {
  vi.mocked(api.organization).mockRejectedValue(new ApiError(403, 'forbidden', 'forbidden'));
  view();
  await screen.findByText('이 조직을 볼 수 없습니다');
  expect(screen.getByRole('link', { name: '홈으로 가기' })).toHaveAttribute('href', '/');
});

const workspace = {
  id: 'workspace-one',
  name: '공동 연구',
  owner_id: 'target',
  owner_name: '소유자',
  member_count: 2,
  created_at: organization.created_at,
  my_role: null,
};

it('joins only after confirmation and turns the row into an open link', async () => {
  vi.spyOn(organizationsApi, 'workspaces').mockResolvedValue([workspace]);
  const join = vi.spyOn(organizationsApi, 'joinAsAdmin').mockResolvedValue(undefined);
  view('workspaces');
  await fireEvent.click(await screen.findByRole('button', { name: '관리자로 참여' }));
  expect(join).not.toHaveBeenCalled();
  await fireEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: '관리자로 참여' }),
  );
  await screen.findByRole('link', { name: '열기' });
  expect(join).toHaveBeenCalledExactlyOnceWith(id, workspace.id);
  expect(screen.queryByRole('button', { name: '관리자로 참여' })).toBeNull();
});

it('excludes self from ownership recipients and preserves a rejected selection', async () => {
  vi.spyOn(organizationsApi, 'workspaces').mockResolvedValue([workspace]);
  vi.mocked(organizationsApi.members).mockResolvedValue([
    ...members,
    { ...firstMember, user_id: 'target', display_name: '받을 멤버', role: 'member' },
  ]);
  const offer = vi
    .spyOn(organizationsApi, 'offerOwnership')
    .mockRejectedValue(new ApiError(409, 'invalid_state', 'not a workspace member'));
  view('workspaces');
  await fireEvent.click(await screen.findByRole('button', { name: '소유권 이전 제안' }));
  await screen.findByRole('option', { name: /받을 멤버/ });
  expect(screen.queryByRole('option', { name: /첫 관리자/ })).toBeNull();
  const recipient = screen.getByRole('combobox', { name: '받을 사람' });
  await fireEvent.change(recipient, { target: { value: 'target' } });
  await fireEvent.click(screen.getByRole('button', { name: '제안 보내기' }));
  await screen.findByText('이 공간의 멤버에게만 제안할 수 있습니다');
  expect(recipient).toHaveValue('target');
  expect(recipient).toHaveAccessibleDescription('이 공간의 멤버에게만 제안할 수 있습니다');
  expect(offer).toHaveBeenCalledExactlyOnceWith(id, workspace.id, 'target');
});

it('restores directly and keeps a retention conflict beside the list', async () => {
  vi.spyOn(organizationsApi, 'workspaces').mockResolvedValue([
    { ...workspace, deleted_at: organization.created_at },
  ]);
  vi.spyOn(organizationsApi, 'restoreWorkspace').mockRejectedValue(
    new ApiError(409, 'invalid_state', '보존 기간이 지났습니다'),
  );
  view('workspaces');
  await fireEvent.click(await screen.findByRole('button', { name: '복원' }));
  await screen.findByText('보존 기간이 지났습니다');
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(screen.getByRole('button', { name: '복원' })).toBeEnabled();
});

it('converts file units to bytes, preserves input on 409 and guards tab departure', async () => {
  const settings = vi
    .spyOn(organizationsApi, 'settings')
    .mockRejectedValue(new ApiError(409, 'invalid_state', '설정을 다시 확인하세요'));
  view('settings');
  await screen.findByRole('textbox', { name: '이름' });
  await fireEvent.click(screen.getByRole('checkbox', { name: '파일 하나 최대 크기 설정 안 함' }));
  const size = screen.getByRole('textbox', { name: '파일 하나 최대 크기' });
  await fireEvent.input(size, { target: { value: '1.5' } });
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  await screen.findByText('설정을 다시 확인하세요');
  expect(size).toHaveValue('1.5');
  expect(settings).toHaveBeenCalledExactlyOnceWith(id, { max_file_bytes: 1572864 });
  router.navigate(organizationPath(id));
  const leave = await screen.findByRole('alertdialog');
  expect(leave).toHaveTextContent('저장하지 않은');
  await fireEvent.click(within(leave).getByRole('button', { name: '계속 편집' }));
  expect(size).toHaveValue('1.5');
});

it('maps settings 400 fields and rejects a blank name before sending', async () => {
  const settings = vi
    .spyOn(organizationsApi, 'settings')
    .mockRejectedValue(
      new ApiError(400, 'bad_request', '이름이 너무 깁니다', undefined, undefined, 'name'),
    );
  view('settings');
  const name = await screen.findByRole('textbox', { name: '이름' });
  await fireEvent.input(name, { target: { value: ' ' } });
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  expect(settings).not.toHaveBeenCalled();
  expect(name).toHaveAccessibleDescription('이름을 입력하세요');
  await fireEvent.input(name, { target: { value: '새 조직' } });
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  await waitFor(() => {
    expect(name).toHaveAccessibleDescription('이름이 너무 깁니다');
  });
  expect(name).toHaveValue('새 조직');
});

it('preserves a one-byte file limit when editing only the organization name', async () => {
  vi.mocked(api.organization).mockResolvedValue({ ...organization, limits: { max_file_bytes: 1 } });
  const settings = vi
    .spyOn(organizationsApi, 'settings')
    .mockResolvedValue({ ...organization, name: '새 이름', limits: { max_file_bytes: 1 } });
  view('settings');
  const name = await screen.findByRole('textbox', { name: '이름' });
  expect(screen.getByRole('button', { name: '저장' })).toBeDisabled();
  await fireEvent.input(name, { target: { value: '새 이름' } });
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  await waitFor(() => {
    expect(settings).toHaveBeenCalledExactlyOnceWith(id, { name: '새 이름' });
  });
});

it('recognizes the four organization routes and defaults to members', () => {
  for (const tab of ['members', 'invitations', 'workspaces', 'settings'] as const)
    expect(parseRoute(organizationPath(id, tab))).toEqual({
      name: 'organization',
      organizationId: id,
      tab,
    });
  expect(parseRoute(`/o/${id}`)).toEqual({
    name: 'organization',
    organizationId: id,
    tab: 'members',
  });
  expect(parseRoute('/admin')).toEqual({ name: 'not-found' });
});

it('shows four tabs and member email to an administrator', async () => {
  render(OrganizationScreen, {
    organizationId: id,
    tab: 'members',
    meId: 'user-a',
    onchanged: () => undefined,
  });
  await screen.findByText('admin@example.test');
  expect(screen.getAllByRole('tab')).toHaveLength(4);
  expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('시험 조직');
});

it('shows only the member tab without email or actions to a member', async () => {
  vi.mocked(api.organization).mockResolvedValue({ ...organization, role: 'member' });
  render(OrganizationScreen, {
    organizationId: id,
    tab: 'members',
    meId: 'user-a',
    onchanged: () => undefined,
  });
  await screen.findByText('첫 관리자');
  expect(screen.getAllByRole('tab')).toHaveLength(1);
  expect(screen.queryByText('admin@example.test')).toBeNull();
  expect(screen.queryByRole('button', { name: /동작/ })).toBeNull();
});

it('pushes history when switching tabs', async () => {
  const push = vi.spyOn(window.history, 'pushState');
  render(OrganizationScreen, {
    organizationId: id,
    tab: 'members',
    meId: 'user-a',
    onchanged: () => undefined,
  });
  await screen.findByText('첫 관리자');
  await fireEvent.click(screen.getByRole('tab', { name: '초대' }));
  expect(push).toHaveBeenCalledWith(null, '', organizationPath(id, 'invitations'));
});
