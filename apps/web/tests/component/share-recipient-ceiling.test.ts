import type { Workspace } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import DocumentActions from '$lib/features/documents/DocumentActions.svelte';
import MembersScreen from '$lib/features/members/MembersScreen.svelte';
import { me } from '$lib/me.svelte';
import { org } from '$lib/org.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { ACCESS_DOCUMENT } from '../../stories/fixtures/access';

const workspace: Workspace = {
  id: 'ws-1',
  organization_id: 'org-1',
  name: 'Space',
  access: 'owner',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};
const recipient = {
  user_id: 'outside-1',
  email: 'outside@example.test',
  display_name: 'Outside',
  recipient_role: 'outside_member',
  max_permission: 'read',
};

beforeEach(() => {
  localStorage.clear();
  me.userId = 'owner-1';
  org.currentId = 'org-1';
  vi.spyOn(api, 'document').mockResolvedValue(ACCESS_DOCUMENT);
  vi.spyOn(api, 'shares').mockResolvedValue([]);
  vi.spyOn(api, 'publicLinks').mockResolvedValue([]);
  vi.stubGlobal(
    'fetch',
    vi.fn().mockImplementation(() =>
      Promise.resolve(
        new Response(JSON.stringify(recipient), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        }),
      ),
    ),
  );
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  me.userId = '';
  org.currentId = undefined;
  localStorage.clear();
});

for (const [locale, role, cap] of [
  ['ko', '공간 밖 조직원', '공유 상한: 읽기'],
  ['en', 'Outside workspace member', 'Sharing limit: Read'],
] as const) {
  it(`hides Edit for an outside member and shows the recipient role and cap (${locale})`, async () => {
    await setLocale(locale, { reload: false });
    render(DocumentActions, {
      doc: ACCESS_DOCUMENT,
      workspaces: [workspace],
      onchanged: () => undefined,
    });
    await fireEvent.click(screen.getByRole('button', { name: m.documents_share_settings() }));
    await fireEvent.input(await screen.findByLabelText(m.documents_email()), {
      target: { value: recipient.email },
    });
    await fireEvent.keyDown(screen.getByRole('button', { name: m.documents_share_permission() }), {
      key: 'ArrowDown',
    });
    await screen.findByRole('option', { name: m.documents_permission_read() });
    expect(screen.queryByRole('option', { name: m.documents_permission_write() })).toBeNull();
    expect(await screen.findByText(role)).toBeInTheDocument();
    expect(screen.getByText(cap)).toBeInTheDocument();
  });

  it(`shows the calculated role and cap for a stored write share (${locale})`, async () => {
    await setLocale(locale, { reload: false });
    const share = {
      ...recipient,
      id: 'share-1',
      grantee_user_id: recipient.user_id,
      permission: 'write' as const,
    };
    vi.mocked(api.shares).mockResolvedValue([share]);
    render(DocumentActions, {
      doc: ACCESS_DOCUMENT,
      workspaces: [workspace],
      onchanged: () => undefined,
    });
    await fireEvent.click(screen.getByRole('button', { name: m.documents_share_settings() }));
    const list = await screen.findByTestId('share-list');
    expect(await within(list).findByText(role)).toBeInTheDocument();
    expect(within(list).getByText(cap)).toBeInTheDocument();
    expect(within(list).queryByText(m.documents_permission_write())).toBeNull();
  });

  it(`explains the outside member role and ceiling on the members screen (${locale})`, async () => {
    await setLocale(locale, { reload: false });
    vi.spyOn(api, 'members').mockResolvedValue({
      can_manage_members: true,
      members: [],
      my_role: 'admin',
      i_am_owner: true,
      pending_owner: null,
    });
    vi.spyOn(api, 'groups').mockResolvedValue([]);
    vi.spyOn(api, 'invitations').mockResolvedValue([]);
    render(MembersScreen, { workspace, meId: me.userId, onchanged: () => undefined });
    expect(await screen.findByText(role)).toBeInTheDocument();
    expect(
      screen.getByText(
        locale === 'ko'
          ? '공유받은 공간 문서만 읽을 수 있습니다. 수정하려면 공간에 초대하세요.'
          : 'Can read shared workspace documents. Invite them to the workspace to allow editing.',
      ),
    ).toBeInTheDocument();
  });
}
