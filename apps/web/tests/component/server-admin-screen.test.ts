import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { serverAdminApi } from '$lib/features/server-admin/api';
import type { ServerOrganization } from '$lib/features/server-admin/model';
import ServerAdminScreen from '$lib/features/server-admin/ServerAdminScreen.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';
import { ApiError, NetworkError } from '$lib/shared/api';

const createdAt = '2026-10-01T00:00:00Z';
const organization = {
  id: 'org',
  name: '합성 조직',
  member_count: 50,
  workspace_count: 3,
  created_at: createdAt,
};
const admins = [
  { user_id: 'user-a', display_name: '현재 관리자', created_at: createdAt },
  { user_id: 'user-b', display_name: '다른 관리자', created_at: createdAt },
];

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'user-a';
  vi.spyOn(serverAdminApi, 'organizations').mockResolvedValue([organization]);
  vi.spyOn(serverAdminApi, 'admins').mockResolvedValue(admins);
  router.replace('/admin/organizations');
});
afterEach(() => {
  vi.restoreAllMocks();
  router.guard = undefined;
  me.userId = '';
});

function view(isServerAdmin = true, tab: 'organizations' | 'admins' = 'organizations') {
  return render(ServerAdminScreen, { isServerAdmin, meId: 'user-a', tab, oncreated: vi.fn() });
}
async function openCreate() {
  await fireEvent.click(await screen.findByRole('button', { name: '조직 만들기' }));
  const dialog = await screen.findByRole('dialog', { name: '조직 만들기' });
  await within(dialog).findByRole('option', { name: '현재 관리자' });
  return dialog;
}

it('denies a direct route without server administration and does not load metadata', async () => {
  view(false);
  expect(await screen.findByText('이 조직을 볼 수 없습니다')).toBeVisible();
  expect(screen.getByRole('link', { name: '홈으로 가기' })).toHaveAttribute('href', '/');
  expect(screen.queryByRole('tab')).toBeNull();
  expect(screen.queryByRole('button', { name: '조직 만들기' })).toBeNull();
  expect(serverAdminApi.organizations).not.toHaveBeenCalled();
  expect(serverAdminApi.admins).not.toHaveBeenCalled();
});

it('lists only organization metadata, normalizes /admin and pushes tab navigation', async () => {
  router.replace('/admin');
  view();
  await screen.findByText('합성 조직');
  expect(router.path).toBe('/admin/organizations');
  const list = screen.getByRole('list', { name: '조직' });
  expect(list).toHaveTextContent('멤버 50명');
  expect(list).toHaveTextContent('공간 3곳');
  expect(list).toHaveTextContent('만든 날');
  expect(within(list).queryByRole('button')).toBeNull();
  expect(within(list).queryByRole('link')).toBeNull();
  const push = vi.spyOn(window.history, 'pushState');
  await fireEvent.click(screen.getByRole('tab', { name: '서버 관리자' }));
  expect(push).toHaveBeenCalledWith(null, '', '/admin/admins');
});

it('lists server administrators and appointment dates without mutation controls', async () => {
  view(true, 'admins');
  await screen.findByText('현재 관리자');
  const list = screen.getByRole('list', { name: '서버 관리자' });
  expect(list).toHaveTextContent('지정일');
  expect(within(list).queryByRole('button')).toBeNull();
  expect(screen.queryByRole('button', { name: '조직 만들기' })).toBeNull();
});

it.each([403, 404])(
  'uses the common unavailable state for a server denial (%s)',
  async (status) => {
    vi.mocked(serverAdminApi.organizations).mockRejectedValue(
      new ApiError(status, 'forbidden', 'denied'),
    );
    view();
    expect(await screen.findByText('이 조직을 볼 수 없습니다')).toBeVisible();
    expect(screen.getByRole('link', { name: '홈으로 가기' })).toBeVisible();
    expect(screen.queryByRole('button', { name: '조직 만들기' })).toBeNull();
  },
);

it('shows a loading skeleton, an inline failure with retry and an empty list with create', async () => {
  let rejectLoad: (reason: unknown) => void = () => undefined;
  vi.mocked(serverAdminApi.organizations).mockImplementationOnce(
    () =>
      new Promise((_resolve, reject) => {
        rejectLoad = reject;
      }),
  );
  vi.mocked(serverAdminApi.organizations).mockResolvedValueOnce([]);
  view();
  expect(screen.getByLabelText('불러오는 중…')).toHaveAttribute('aria-busy', 'true');
  rejectLoad(new NetworkError('offline'));
  await fireEvent.click(await screen.findByRole('button', { name: '다시 시도' }));
  expect(await screen.findByText('아직 조직이 없습니다')).toBeVisible();
  expect(screen.getByRole('button', { name: '조직 만들기' })).toBeEnabled();
});

it.each(['success', 'network error', '403', '404'] as const)(
  'ignores a superseded initial %s after the creation refresh completes',
  async (outcome) => {
    let resolveFirst: (list: ServerOrganization[]) => void = () => undefined;
    let rejectFirst: (reason: unknown) => void = () => undefined;
    const first = new Promise<ServerOrganization[]>((resolve, reject) => {
      resolveFirst = resolve;
      rejectFirst = reject;
    });
    const created = { ...organization, id: 'created', name: '새로 만든 조직' };
    vi.mocked(serverAdminApi.organizations)
      .mockImplementationOnce(() => first)
      .mockResolvedValueOnce([organization, created]);
    vi.spyOn(serverAdminApi, 'createOrganization').mockResolvedValue({ id: created.id });
    view();
    const dialog = await openCreate();
    await fireEvent.input(within(dialog).getByRole('textbox', { name: '이름' }), {
      target: { value: created.name },
    });
    await fireEvent.click(within(dialog).getByRole('button', { name: '조직 만들기' }));
    await screen.findByText(created.name);
    expect(serverAdminApi.organizations).toHaveBeenCalledTimes(2);

    if (outcome === 'success') resolveFirst([organization]);
    else if (outcome === 'network error') rejectFirst(new NetworkError('old request failed'));
    else rejectFirst(new ApiError(outcome === '403' ? 403 : 404, 'forbidden', 'old denial'));
    await first.catch(() => undefined);
    await tick();

    const list = screen.getByRole('list', { name: '조직' });
    expect(list).toHaveTextContent(created.name);
    expect(within(list).getAllByRole('listitem')).toHaveLength(2);
    expect(screen.queryByText('이 조직을 볼 수 없습니다')).toBeNull();
    expect(screen.queryByRole('button', { name: '다시 시도' })).toBeNull();
  },
);

it.each([
  {
    locale: 'ko',
    create: '조직 만들기',
    label: '첫 관리자',
    help: '첫 관리자는 조직에서 다른 사람을 초대하고 관리자 역할을 넘길 수 있습니다.',
  },
  {
    locale: 'en',
    create: 'Create organization',
    label: 'First administrator',
    help: 'The first administrator can invite others to the organization and hand over the administrator role.',
  },
] as const)(
  'shows the specified help and defaults the first administrator to me in $locale',
  async ({ locale, create, label, help }) => {
    await setLocale(locale, { reload: false });
    view();
    await fireEvent.click(await screen.findByRole('button', { name: create }));
    const dialog = await screen.findByRole('dialog', { name: create });
    await within(dialog).findByRole('option', { name: '현재 관리자' });
    expect(within(dialog).getByRole('combobox', { name: label })).toHaveValue('user-a');
    expect(within(dialog).getByRole('combobox', { name: label })).toHaveAccessibleDescription(help);
    expect(within(dialog).getByRole('button', { name: create })).toBeEnabled();
  },
);

it('keeps the request ID and input after a lost response and prevents concurrent creation', async () => {
  const mintedIds = new Set<string>();
  const randomUUID = crypto.randomUUID.bind(crypto);
  vi.spyOn(crypto, 'randomUUID').mockImplementation(() => {
    const id = randomUUID();
    mintedIds.add(id);
    return id;
  });
  let resolveCreate: (value: { id: string }) => void = () => undefined;
  const create = vi
    .spyOn(serverAdminApi, 'createOrganization')
    .mockRejectedValueOnce(new NetworkError('response lost'))
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveCreate = resolve;
        }),
    );
  view();
  const dialog = await openCreate();
  const idsAtOpen = [...mintedIds];
  const name = within(dialog).getByRole('textbox', { name: '이름' });
  await fireEvent.input(name, { target: { value: '새 조직' } });
  await fireEvent.change(within(dialog).getByRole('combobox', { name: '첫 관리자' }), {
    target: { value: 'user-b' },
  });
  const submit = within(dialog).getByRole('button', { name: '조직 만들기' });
  await fireEvent.click(submit);
  await waitFor(() => {
    expect(create).toHaveBeenCalledTimes(1);
  });
  await waitFor(() => {
    expect(submit).toBeEnabled();
  });
  expect(name).toHaveValue('새 조직');
  await fireEvent.click(submit);
  await fireEvent.submit(within(dialog).getByRole('form', { name: '조직 만들기' }));
  expect(create).toHaveBeenCalledTimes(2);
  const first = create.mock.calls[0]?.[0];
  expect(first).toEqual({
    request_id: first?.request_id,
    name: '새 조직',
    first_admin_user_id: 'user-b',
  });
  expect(first?.request_id).toMatch(/^[0-9a-f-]{36}$/);
  expect(idsAtOpen).toContain(first?.request_id);
  expect(create.mock.calls[1]?.[0]).toEqual(first);
  expect(submit).toBeDisabled();
  resolveCreate({ id: 'created' });
  await waitFor(() => {
    expect(screen.queryByRole('dialog')).toBeNull();
  });
  const next = await openCreate();
  await fireEvent.input(within(next).getByRole('textbox', { name: '이름' }), {
    target: { value: '다음 조직' },
  });
  create.mockResolvedValueOnce({ id: 'next' });
  await fireEvent.click(within(next).getByRole('button', { name: '조직 만들기' }));
  await waitFor(() => {
    expect(create).toHaveBeenCalledTimes(3);
  });
  expect(create.mock.calls[2]?.[0].request_id).not.toBe(first?.request_id);
});

it.each(['name', 'first_admin_user_id'])(
  'attaches a server 400 to %s and keeps the input',
  async (field) => {
    vi.spyOn(serverAdminApi, 'createOrganization').mockRejectedValue(
      new ApiError(400, 'bad_request', 'invalid input', undefined, undefined, field),
    );
    view();
    const dialog = await openCreate();
    const name = within(dialog).getByRole('textbox', { name: '이름' });
    await fireEvent.input(name, { target: { value: '입력 보존' } });
    await fireEvent.click(within(dialog).getByRole('button', { name: '조직 만들기' }));
    const input =
      field === 'name' ? name : within(dialog).getByRole('combobox', { name: '첫 관리자' });
    await waitFor(() => expect(input).toHaveAttribute('aria-invalid', 'true'));
    expect(input).toHaveAccessibleDescription();
    expect(name).toHaveValue('입력 보존');
  },
);

it('keeps an English conflict diagnostic inside the dialog for retry', async () => {
  await setLocale('en', { reload: false });
  vi.spyOn(serverAdminApi, 'createOrganization').mockRejectedValue(
    new ApiError(409, 'conflict', 'This request was already used with different input.'),
  );
  view();
  await fireEvent.click(await screen.findByRole('button', { name: 'Create organization' }));
  const dialog = screen.getByRole('dialog');
  await within(dialog).findByRole('option', { name: '현재 관리자' });
  await fireEvent.input(within(dialog).getByRole('textbox', { name: 'Name' }), {
    target: { value: 'Retained' },
  });
  await fireEvent.click(within(dialog).getByRole('button', { name: 'Create organization' }));
  expect(
    await within(dialog).findByText('This request was already used with different input.'),
  ).toBeVisible();
  expect(within(dialog).getByRole('textbox', { name: 'Name' })).toHaveValue('Retained');
});

it.each(['cancel', 'Escape', 'close'])(
  'keeps the create dialog and input after declining a %s close',
  async (action) => {
    view();
    const dialog = await openCreate();
    await fireEvent.input(within(dialog).getByRole('textbox', { name: '이름' }), {
      target: { value: '작성 중' },
    });
    if (action === 'Escape')
      await fireEvent.keyDown(within(dialog).getByRole('textbox', { name: '이름' }), {
        key: 'Escape',
      });
    else
      await fireEvent.click(
        within(dialog).getByRole('button', { name: action === 'close' ? '닫기' : '취소' }),
      );
    expect(await screen.findByRole('alertdialog')).toBeVisible();
    await fireEvent.click(screen.getByRole('button', { name: '계속 편집' }));
    expect(within(dialog).getByRole('textbox', { name: '이름' })).toHaveValue('작성 중');
    await fireEvent.click(within(dialog).getByRole('button', { name: '취소' }));
    await screen.findByRole('alertdialog');
    await fireEvent.click(screen.getByRole('button', { name: '변경 버리고 닫기' }));
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).toBeNull();
    });
  },
);

it('retries administrator loading in the create dialog without clearing the name', async () => {
  vi.mocked(serverAdminApi.admins).mockRejectedValueOnce(new NetworkError('offline'));
  view();
  await fireEvent.click(await screen.findByRole('button', { name: '조직 만들기' }));
  const dialog = screen.getByRole('dialog');
  const name = within(dialog).getByRole('textbox', { name: '이름' });
  await fireEvent.input(name, { target: { value: '그대로 남길 이름' } });
  const submit = within(dialog).getByRole('button', { name: '조직 만들기' });
  expect(submit).toBeDisabled();
  expect(submit).toHaveAccessibleDescription();
  await fireEvent.click(await within(dialog).findByRole('button', { name: '다시 시도' }));
  await within(dialog).findByRole('option', { name: '현재 관리자' });
  expect(name).toHaveValue('그대로 남길 이름');
  expect(submit).toBeEnabled();
});

it('explains why an empty administrator list disables creation', async () => {
  vi.mocked(serverAdminApi.admins).mockResolvedValue([]);
  view();
  await fireEvent.click(await screen.findByRole('button', { name: '조직 만들기' }));
  const dialog = screen.getByRole('dialog');
  await within(dialog).findByText('등록된 서버 관리자가 없습니다');
  const submit = within(dialog).getByRole('button', { name: '조직 만들기' });
  expect(submit).toBeDisabled();
  expect(submit).toHaveAccessibleDescription('등록된 서버 관리자가 없습니다');
});

it('renders a failed administrator tab inline and supports retry to an empty list', async () => {
  vi.mocked(serverAdminApi.admins)
    .mockRejectedValueOnce(new NetworkError('offline'))
    .mockResolvedValueOnce([]);
  view(true, 'admins');
  await fireEvent.click(await screen.findByRole('button', { name: '다시 시도' }));
  expect(await screen.findByText('등록된 서버 관리자가 없습니다')).toBeVisible();
});

it.each([403, 404])(
  'denies the screen when loading administrators or submitting returns %s',
  async (status) => {
    const denied = new ApiError(status, 'forbidden', 'denied');
    const create = vi.spyOn(serverAdminApi, 'createOrganization').mockRejectedValue(denied);
    view();
    const dialog = await openCreate();
    await fireEvent.input(within(dialog).getByRole('textbox', { name: '이름' }), {
      target: { value: '조직' },
    });
    await fireEvent.click(within(dialog).getByRole('button', { name: '조직 만들기' }));
    await screen.findByText('이 조직을 볼 수 없습니다');
    expect(create).toHaveBeenCalledOnce();
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.getByRole('link', { name: '홈으로 가기' })).toBeVisible();
  },
);

it.each([403, 404])(
  'denies the screen when the first-administrator lookup returns %s',
  async (status) => {
    vi.mocked(serverAdminApi.admins).mockRejectedValue(new ApiError(status, 'forbidden', 'denied'));
    view();
    await fireEvent.click(await screen.findByRole('button', { name: '조직 만들기' }));
    await screen.findByText('이 조직을 볼 수 없습니다');
    expect(screen.queryByRole('dialog')).toBeNull();
  },
);

it('validates a blank name beside the input before creating', async () => {
  const create = vi.spyOn(serverAdminApi, 'createOrganization');
  view();
  const dialog = await openCreate();
  const name = within(dialog).getByRole('textbox', { name: '이름' });
  await fireEvent.input(name, { target: { value: '   ' } });
  await fireEvent.click(within(dialog).getByRole('button', { name: '조직 만들기' }));
  expect(name).toHaveAttribute('aria-invalid', 'true');
  expect(name).toHaveAccessibleDescription('이름을 입력하세요');
  expect(create).not.toHaveBeenCalled();
});
