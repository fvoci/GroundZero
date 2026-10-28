<script lang="ts">
  // 멤버·초대·그룹·소유권 화면 (D-20, D-51). 권한 판단은 서버가 하고, 화면은 가능한 동작만 보여준다.
  import { formatInstant } from '$lib/display';
  import type { Group, Invitation, MembersView, Role, Workspace } from '@fvoci/api-client';
  import { onMount, untrack } from 'svelte';

  import { api, ApiError } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { router } from '$lib/router.svelte';
  import { Badge } from '$lib/ui/badge';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import { me } from '$lib/me.svelte';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';

  import RoleSelect from './RoleSelect.svelte';
  import { DISPLAY_ROLES, roleLabel } from './roles';

  interface Props {
    workspace: Workspace;
    /** 현재 사용자 id. 인계 수락 버튼 표시에 쓴다. */
    meId: string;
    onchanged: () => void;
  }

  const { workspace, meId, onchanged }: Props = $props();

  let view = $state<MembersView | undefined>(undefined);
  let invitations = $state<Invitation[]>([]);
  let groups = $state<Group[]>([]);
  let error = $state<string | undefined>(undefined);
  let inviteEmail = $state('');
  let inviteRole = $state<Role>('member');
  let groupName = $state('');
  // 초대·그룹 만들기 실패는 그 입력 칸 옆에 보이고 입력은 남긴다(D-82).
  let inviteError = $state<string | undefined>(undefined);
  // 초대를 보내는 중. 그동안 다시 보내지 않는다(Enter 두 번이면 같은 초대가 두 번 나갔다).
  let inviting = $state(false);
  let grouping = $state(false);
  let groupError = $state<string | undefined>(undefined);
  let deleteName = $state('');
  let deleteError = $state<string | undefined>(undefined);
  let deleting = $state(false);

  async function deleteWorkspace(): Promise<void> {
    if (deleting) return;
    deleting = true;
    deleteError = undefined;
    try {
      await api.deleteWorkspace(workspace.id, deleteName);
      // 지운 뒤의 이동을 이탈 경고가 막지 않게 한다. 삭제 이름의 가드는 아직 `deleting`(busy)이라 묻지 않고 막으므로
      // 그 가드는 건너뛰고(`proceed`), 다른 입력 칸의 가드는 그대로 거친다(R5 delta F-1).
      deleteName = '';
      onchanged();
      deleteDraft.leave.proceed(() => {
        router.navigate('/trash');
      });
    } catch (e: unknown) {
      deleteError = errorText(e, m.members_delete_failed());
    } finally {
      deleting = false;
    }
  }
  let confirm = $state<{ title: string; body: string; run: () => Promise<unknown> } | undefined>(
    undefined,
  );

  // 관리 가능 여부는 서버가 접근 등급으로 계산한 칸이다(D-113). 역할은 표시에만 쓴다.
  const isAdmin = $derived(view?.can_manage_members === true);

  const inviteDraft = createFormDraft<{ email: string; role: Role }>({
    name: () => `members-invite:${workspace.id}`,
    snapshot: () => ({ email: inviteEmail, role: inviteRole }),
    parse: (value) => {
      if (
        typeof value !== 'object' ||
        value === null ||
        !('email' in value) ||
        typeof value.email !== 'string' ||
        !('role' in value) ||
        (value.role !== 'admin' && value.role !== 'member' && value.role !== 'readonly')
      )
        return undefined;
      return { email: value.email, role: value.role };
    },
    restore: (value) => {
      inviteEmail = value.email;
      inviteRole = value.role;
      inviteError = undefined;
    },
    reset: () => {
      inviteEmail = '';
      inviteRole = 'member';
      inviteError = undefined;
    },
    dirty: () => inviteEmail !== '',
    busy: () => inviting,
  });
  const groupDraft = createFormDraft<string>({
    name: () => `members-group:${workspace.id}`,
    snapshot: () => groupName,
    parse: (value) => (typeof value === 'string' ? value : undefined),
    restore: (value) => {
      groupName = value;
      groupError = undefined;
    },
    reset: () => {
      groupName = '';
      groupError = undefined;
    },
    dirty: () => groupName !== '',
    busy: () => grouping,
  });
  // 삭제 재확인 이름은 저장하지 않는다(R3 A). 화면에 있는 동안만 두고 떠날 때만 경고하며, 사용자·워크스페이스가
  // 바뀌면 지운다.
  const deleteDraft = createFormDraft<undefined>({
    name: () => `members-delete:${workspace.id}`,
    snapshot: () => undefined,
    parse: () => undefined,
    restore: () => undefined,
    reset: () => {
      deleteName = '';
      deleteError = undefined;
    },
    dirty: () => false,
    screenOnly: () => deleteName !== '',
    busy: () => deleting,
  });
  onMount(() => {
    const detachInvite = inviteDraft.attach();
    const detachGroup = groupDraft.attach();
    const detachDelete = deleteDraft.attach();
    return () => {
      detachDelete();
      detachGroup();
      detachInvite();
    };
  });

  async function invite(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (inviting || !isAdmin) return;
    const attempt = inviteDraft.capture();
    if (attempt === undefined) return;
    const wsId = workspace.id;
    const email = inviteEmail;
    const role = inviteRole;
    inviteError = undefined;
    inviting = true;
    try {
      await api.invite(wsId, email, role);
      if (!inviteDraft.isOwner(attempt)) return;
      // 일반 완료는 토스트다(D-82).
      if (inviteDraft.complete(attempt)) notifyDone(m.members_invited({ email }));
      await load(wsId);
      if (inviteDraft.isOwner(attempt)) onchanged();
    } catch (e: unknown) {
      if (!inviteDraft.isOwner(attempt)) return;
      if (e instanceof ApiError && e.status === 409) {
        await load(wsId);
        if (inviteDraft.isOwner(attempt)) onchanged();
      }
      if (inviteDraft.isCurrent(attempt)) inviteError = errorText(e, m.members_request_failed());
    } finally {
      if (inviteDraft.isOwner(attempt)) inviting = false;
    }
  }

  async function createGroup(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (grouping || !isAdmin) return;
    const attempt = groupDraft.capture();
    if (attempt === undefined) return;
    const wsId = workspace.id;
    const name = groupName;
    groupError = undefined;
    grouping = true;
    try {
      await api.createGroup(wsId, name);
      if (!groupDraft.isOwner(attempt)) return;
      groupDraft.complete(attempt);
      await load(wsId);
      if (groupDraft.isOwner(attempt)) onchanged();
    } catch (e: unknown) {
      if (!groupDraft.isOwner(attempt)) return;
      if (e instanceof ApiError && e.status === 409) {
        await load(wsId);
        if (groupDraft.isOwner(attempt)) onchanged();
      }
      if (groupDraft.isCurrent(attempt)) groupError = errorText(e, m.members_request_failed());
    } finally {
      if (groupDraft.isOwner(attempt)) grouping = false;
    }
  }

  function nameOf(userId: string): string {
    return view?.members.find((member) => member.user_id === userId)?.display_name ?? userId;
  }

  // 읽기 실패는 요청(역할 바꾸기·그룹 삭제 등) 실패와 따로 둔다. 읽기 실패에만 다시 읽기 단추가 있고, 그 뒤 요청이
  // 실패해도 다시 읽기 단추는 남는다.
  let loadError = $state<string | undefined>(undefined);

  let loadSequence = 0;
  async function load(wsId: string = workspace.id): Promise<void> {
    const sequence = ++loadSequence;
    const userId = me.userId;
    const current = (): boolean =>
      sequence === loadSequence && workspace.id === wsId && me.userId === userId;
    try {
      const v = await api.members(wsId);
      if (!current()) return;
      view = v;
      const loadedGroups = await api.groups(wsId);
      if (!current()) return;
      groups = loadedGroups;
      const loadedInvitations = v.can_manage_members ? await api.invitations(wsId) : [];
      if (!current()) return;
      invitations = loadedInvitations;
      error = undefined;
      loadError = undefined;
    } catch (e: unknown) {
      if (current()) loadError = errorText(e, m.members_load_failed());
    }
  }

  /**
   * 요청을 보내고 성공했는지 돌려준다(실패해도 던지지 않는다). `fail`이 있으면 실패 문구를 화면 위 알림 대신
   * 그쪽(입력 칸 옆)에 보인다.
   */
  async function act(
    action: () => Promise<unknown>,
    done?: string,
    fail?: (text: string) => void,
  ): Promise<boolean> {
    try {
      await action();
    } catch (e: unknown) {
      // 그사이 상태가 바뀌었으면(409: 이미 수락·철회된 초대 등) 지금 상태를 먼저 다시 읽어 보인다. 다시 읽기가
      // 성공하면 위 알림을 지우므로 이유는 그 뒤에 둔다.
      if (e instanceof ApiError && e.status === 409) {
        await load();
        onchanged();
      }
      const text = errorText(e, m.members_request_failed());
      if (fail === undefined) error = text;
      else fail(text);
      return false;
    }
    if (done !== undefined) notifyDone(done);
    await load();
    onchanged();
    return true;
  }

  // 다른 워크스페이스로 바뀌면(또는 사용자가 바뀌면) 다시 읽는다. 초대·그룹 만들기 뒤 앱이 같은 id의 새 객체를 넘기는
  // 것(이름 등 갱신, `onchanged`)으로는 다시 돌지 않는다: 돌면 화면을 비워 성공 안내와 입력 요소(초점)를 지웠다
  // (R5 delta2 fixes §7). 같은 id의 갱신은 `workspace`를 직접 그리는 곳에 그대로 반영된다.
  // 문자열 열쇠라 같은 id·같은 사용자면 값이 같아 효과가 다시 돌지 않는다.
  const loadKey = $derived(JSON.stringify([workspace.id, me.userId]));
  $effect(() => {
    if (loadKey === '') return;
    untrack(() => {
      reset(workspace.id);
    });
  });
  function reset(id: string): void {
    view = undefined;
    groups = [];
    invitations = [];
    inviting = false;
    grouping = false;
    error = undefined;
    loadError = undefined;
    void load(id);
  }
</script>

<section class="flex flex-col gap-8" aria-labelledby="members-heading">
  <h1 id="members-heading" class="text-xl font-semibold">
    {m.members_heading({ workspace: workspace.name })}
  </h1>

  <section
    aria-labelledby="role-limits-heading"
    class="flex flex-col gap-2 rounded border p-3 text-sm"
  >
    <h2 id="role-limits-heading" class="font-semibold">{m.members_role_limits()}</h2>
    <dl class="grid grid-cols-2 gap-2">
      {#each DISPLAY_ROLES as role (role)}
        <dt>{roleLabel(role)}</dt>
        <dd>
          {role === 'admin' || role === 'member' ? m.members_limit_write() : m.members_limit_read()}
        </dd>
      {/each}
    </dl>
    <p>{m.members_outside_member_hint()}</p>
  </section>

  {#if loadError !== undefined}
    <!-- 멤버·그룹·초대 어느 읽기가 실패해도(목록이 이미 있어도) 다시 읽을 수 있다. -->
    <p role="alert" class="rounded border border-destructive/40 bg-destructive/10 px-3 py-2">
      {loadError}
      <Button
        size="sm"
        variant="outline"
        class="ml-2"
        onclick={() => {
          loadError = undefined;
          void load();
        }}>{m.common_retry()}</Button
      >
    </p>
  {/if}
  {#if error !== undefined}
    <p role="alert" class="rounded border border-destructive/40 bg-destructive/10 px-3 py-2">
      {error}
    </p>
  {/if}

  {#if view === undefined}
    <!-- 첫 읽기가 실패하면 "불러오는 중"을 남기지 않는다(위 오류에 다시 시도가 있다). -->
    {#if loadError === undefined}
      <p aria-live="polite">{m.app_loading()}</p>
    {/if}
  {:else}
    {@const v = view}
    <ul class="flex flex-col divide-y rounded border" data-testid="member-list">
      {#each v.members as member (member.user_id)}
        <li class="flex flex-wrap items-center gap-3 px-3 py-2">
          <span class="font-medium">{member.display_name}</span>
          {#if member.is_owner}<Badge variant="secondary">{m.members_owner()}</Badge>{/if}
          {#if v.pending_owner === member.user_id}<Badge variant="outline"
              >{m.members_pending_owner()}</Badge
            >{/if}
          {#if member.email !== null}
            <span class="text-sm text-muted-foreground">{member.email}</span>
          {/if}
          <span class="ml-auto flex items-center gap-2">
            {#if isAdmin && !member.is_owner}
              <RoleSelect
                value={member.role}
                label={m.members_role_of({ name: member.display_name })}
                onchange={(role: Role) =>
                  void act(
                    () => api.setRole(workspace.id, member.user_id, role),
                    m.members_role_changed({ name: member.display_name, role: roleLabel(role) }),
                  )}
              />
              <Button
                size="sm"
                variant="ghost"
                onclick={() =>
                  (confirm = {
                    title: m.members_remove_title({ name: member.display_name }),
                    body: m.members_remove_body(),
                    run: () => act(() => api.removeMember(workspace.id, member.user_id)),
                  })}>{m.members_remove()}</Button
              >
              {#if v.i_am_owner}
                <Button
                  size="sm"
                  variant="ghost"
                  onclick={() =>
                    (confirm = {
                      title: m.members_transfer_title({ name: member.display_name }),
                      body: m.members_transfer_body(),
                      run: () => act(() => api.offerOwnership(workspace.id, member.user_id)),
                    })}>{m.members_transfer()}</Button
                >
              {/if}
            {:else}
              <span class="text-sm">{roleLabel(member.role)}</span>
            {/if}
          </span>
        </li>
      {/each}
    </ul>

    {@const pendingOwner = v.pending_owner ?? null}
    {#if pendingOwner !== null}
      <div
        class="flex flex-wrap items-center gap-2 rounded border px-3 py-2 text-sm"
        role="region"
        aria-label={m.members_transfer()}
      >
        <span>{m.members_transfer_offered({ name: nameOf(pendingOwner) })}</span>
        {#if v.i_am_owner}
          <Button
            size="sm"
            variant="ghost"
            onclick={() => void act(() => api.cancelOwnershipOffer(workspace.id))}
            >{m.members_cancel_offer()}</Button
          >
        {/if}
        {#if pendingOwner === meId}
          <Button
            size="sm"
            onclick={() =>
              void act(() => api.acceptOwnership(workspace.id), m.members_became_owner())}
            >{m.members_accept_ownership()}</Button
          >
        {/if}
      </div>
    {/if}

    {#if isAdmin}
      <section class="flex flex-col gap-3" aria-labelledby="invite-heading">
        <h2 id="invite-heading" class="font-semibold">{m.members_invite_heading()}</h2>
        <p class="text-sm text-muted-foreground">
          {m.members_invite_hint()}
        </p>
        <!-- 초대·그룹 만들기는 보내는 동안 입력과 되살리기를 잠근다(D-97). 그사이 바뀐 입력은 성공 안내와 실패 알림을
             숨긴다. 역할 선택(bits-ui Select)은 조상 fieldset의 disabled를 보지 않으므로 따로 잠근다. -->
        {#each inviteDraft.offers as offer (offer.key)}
          <fieldset class="contents" disabled={inviting}>
            <FormDraftOffer
              onrestore={() => {
                inviteDraft.restore(offer);
              }}
              ondiscard={() => {
                inviteDraft.discard(offer);
              }}
            />
          </fieldset>
        {/each}
        <fieldset class="min-w-0" disabled={inviting}>
          <form class="flex flex-wrap items-end gap-2" onsubmit={(e) => void invite(e)}>
            <div class="flex min-w-60 flex-1 flex-col gap-1">
              <Label for="invite-email">{m.members_email()}</Label>
              <Input
                id="invite-email"
                type="email"
                bind:value={inviteEmail}
                autocomplete="off"
                required
                aria-invalid={inviteError !== undefined}
                aria-describedby={inviteError !== undefined ? 'invite-email-error' : undefined}
              />
            </div>
            <RoleSelect
              value={inviteRole}
              label={m.members_invite_role()}
              disabled={inviting}
              onchange={(r: Role) => (inviteRole = r)}
            />
            <Button type="submit" disabled={inviting || inviteEmail.trim() === ''}
              >{m.members_invite()}</Button
            >
            {#if inviteError !== undefined}
              <p id="invite-email-error" role="alert" class="w-full text-sm text-destructive">
                {inviteError}
              </p>
            {/if}
          </form>
        </fieldset>
        {#if invitations.length > 0}
          <ul class="flex flex-col gap-1" data-testid="invitation-list">
            {#each invitations as inv (inv.id)}
              <li class="flex items-center gap-2 text-sm">
                <span>{inv.email}</span>
                <Badge variant="outline">{roleLabel(inv.role)}</Badge>
                <span class="text-muted-foreground"
                  >{m.members_invitation_expires({
                    date: formatInstant(inv.expires_at, { dateStyle: 'medium' }),
                  })}</span
                >
                <Button
                  size="sm"
                  variant="ghost"
                  onclick={() => void act(() => api.revokeInvitation(inv.id))}
                  >{m.members_revoke()}</Button
                >
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}

    <section class="flex flex-col gap-3" aria-labelledby="groups-heading">
      <h2 id="groups-heading" class="font-semibold">{m.members_groups_heading()}</h2>
      {#if groups.length === 0}
        <p class="text-sm text-muted-foreground">{m.members_no_groups()}</p>
      {/if}
      {#each groups as group (group.id)}
        <div class="flex flex-col gap-2 rounded border px-3 py-2">
          <div class="flex items-center gap-2">
            <span class="font-medium">{group.name}</span>
            <span class="text-sm text-muted-foreground"
              >{m.members_group_size({ count: group.members.length })}</span
            >
            {#if isAdmin}
              <Button
                size="sm"
                variant="ghost"
                class="ml-auto"
                onclick={() =>
                  (confirm = {
                    title: m.members_delete_group_title({ name: group.name }),
                    body: m.members_delete_group_body(),
                    run: () => act(() => api.deleteGroup(group.id)),
                  })}>{m.members_delete_group()}</Button
              >
            {/if}
          </div>
          <ul class="flex flex-wrap gap-2">
            {#each v.members as member (member.user_id)}
              {@const inGroup = group.members.includes(member.user_id)}
              {#if isAdmin || inGroup}
                <li>
                  <Button
                    size="sm"
                    variant={inGroup ? 'secondary' : 'outline'}
                    aria-pressed={inGroup}
                    disabled={!isAdmin}
                    onclick={() =>
                      void act(() => api.setGroupMember(group.id, member.user_id, !inGroup))}
                    >{member.display_name}</Button
                  >
                </li>
              {/if}
            {/each}
          </ul>
        </div>
      {/each}
      {#if isAdmin}
        {#each groupDraft.offers as offer (offer.key)}
          <fieldset class="contents" disabled={grouping}>
            <FormDraftOffer
              onrestore={() => {
                groupDraft.restore(offer);
              }}
              ondiscard={() => {
                groupDraft.discard(offer);
              }}
            />
          </fieldset>
        {/each}
        <fieldset class="min-w-0" disabled={grouping}>
          <form class="flex flex-wrap items-end gap-2" onsubmit={(e) => void createGroup(e)}>
            <div class="flex flex-1 flex-col gap-1">
              <Label for="group-name">{m.members_new_group_name()}</Label>
              <Input
                id="group-name"
                bind:value={groupName}
                maxlength={200}
                autocomplete="off"
                aria-invalid={groupError !== undefined}
                aria-describedby={groupError !== undefined ? 'group-name-error' : undefined}
              />
            </div>
            <Button type="submit" disabled={grouping || groupName.trim() === ''}
              >{m.members_create_group()}</Button
            >
            {#if groupError !== undefined}
              <p id="group-name-error" role="alert" class="w-full text-sm text-destructive">
                {groupError}
              </p>
            {/if}
          </form>
        </fieldset>
      {/if}
    </section>

    {#if !v.i_am_owner}
      <section class="flex flex-col gap-2" aria-labelledby="leave-heading">
        <h2 id="leave-heading" class="font-semibold">{m.members_leave_heading()}</h2>
        <div>
          <Button
            variant="outline"
            onclick={() =>
              (confirm = {
                title: m.members_leave_title({ workspace: workspace.name }),
                body: m.members_leave_body(),
                run: async () => {
                  await api.leave(workspace.id);
                  onchanged();
                  router.navigate('/');
                },
              })}>{m.members_leave()}</Button
          >
        </div>
      </section>
    {/if}

    {#if isAdmin}
      <section class="flex flex-col gap-2" aria-labelledby="export-heading">
        <h2 id="export-heading" class="font-semibold">{m.members_export_heading()}</h2>
        <p class="text-sm text-muted-foreground">
          {m.members_export_hint()}
        </p>
        <div>
          <Button
            variant="outline"
            href={`/api/workspaces/${encodeURIComponent(workspace.id)}/export`}
            >{m.members_export_zip()}</Button
          >
        </div>
      </section>
    {/if}

    {#if v.i_am_owner}
      <section class="flex flex-col gap-2" aria-labelledby="delete-heading">
        <h2 id="delete-heading" class="font-semibold">{m.members_delete_heading()}</h2>
        <p class="text-sm text-muted-foreground">
          {m.members_delete_hint()}
        </p>
        <form
          class="flex flex-wrap items-end gap-2"
          onsubmit={(e: SubmitEvent) => {
            e.preventDefault();
            void deleteWorkspace();
          }}
        >
          <div class="flex flex-1 flex-col gap-1">
            <Label for="delete-name">{m.members_delete_name_label({ name: workspace.name })}</Label>
            <Input id="delete-name" bind:value={deleteName} autocomplete="off" />
          </div>
          <Button
            type="submit"
            variant="destructive"
            disabled={deleting || deleteName.trim() !== workspace.name.trim()}
            >{m.members_delete()}</Button
          >
        </form>
        {#if deleteError !== undefined}
          <p role="alert" class="text-sm text-destructive">{deleteError}</p>
        {/if}
      </section>
    {/if}
  {/if}
</section>

<Dialog.Root
  open={confirm !== undefined}
  onOpenChange={(open: boolean) => {
    if (!open) confirm = undefined;
  }}
>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>{confirm?.title}</Dialog.Title>
      <Dialog.Description>{confirm?.body}</Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer>
      <Button variant="ghost" onclick={() => (confirm = undefined)}>{m.common_cancel()}</Button>
      <Button
        onclick={() => {
          const run = confirm?.run;
          confirm = undefined;
          if (run !== undefined)
            void run().catch((e: unknown) => {
              error = errorText(e, m.members_request_failed());
            });
        }}>{m.members_confirm()}</Button
      >
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<DiscardChangesConfirm
  open={inviteDraft.leave.open}
  onkeep={inviteDraft.leave.keep}
  ondiscard={inviteDraft.discardAndLeave}
/>
<DiscardChangesConfirm
  open={groupDraft.leave.open}
  onkeep={groupDraft.leave.keep}
  ondiscard={groupDraft.discardAndLeave}
/>
<DiscardChangesConfirm
  open={deleteDraft.leave.open}
  onkeep={deleteDraft.leave.keep}
  ondiscard={deleteDraft.discardAndLeave}
/>
