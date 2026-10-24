<script lang="ts">
  // 멤버·초대·그룹·소유권 화면 (D-20, D-51). 권한 판단은 서버가 하고, 화면은 가능한 동작만 보여준다.
  import { formatInstant } from '$lib/display';
  import type { Group, Invitation, MembersView, Role, Workspace } from '@fvoci/api-client';

  import { api, ApiError } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { router } from '$lib/router.svelte';
  import { Badge } from '$lib/ui/badge';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  import RoleSelect from './RoleSelect.svelte';
  import { roleLabel } from './roles';

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
  let notice = $state<string | undefined>(undefined);
  let inviteEmail = $state('');
  let inviteRole = $state<Role>('writer');
  let groupName = $state('');
  // 초대·그룹 만들기 실패는 그 입력 칸 옆에 보이고 입력은 남긴다(D-82).
  let inviteError = $state<string | undefined>(undefined);
  // 초대를 보내는 중. 그동안 다시 보내지 않는다(Enter 두 번이면 같은 초대가 두 번 나갔다).
  let inviting = $state(false);
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
      onchanged();
      router.navigate('/trash');
    } catch (e: unknown) {
      deleteError = errorText(e, m.members_delete_failed());
    } finally {
      deleting = false;
    }
  }
  let confirm = $state<{ title: string; body: string; run: () => Promise<unknown> } | undefined>(
    undefined,
  );

  const isAdmin = $derived(view?.my_role === 'admin');

  function nameOf(userId: string): string {
    return view?.members.find((member) => member.user_id === userId)?.display_name ?? userId;
  }

  // 읽기 실패는 요청(역할 바꾸기·그룹 삭제 등) 실패와 따로 둔다. 읽기 실패에만 다시 읽기 단추가 있고, 그 뒤 요청이
  // 실패해도 다시 읽기 단추는 남는다.
  let loadError = $state<string | undefined>(undefined);

  async function load(wsId: string = workspace.id): Promise<void> {
    try {
      const v = await api.members(wsId);
      view = v;
      groups = await api.groups(wsId);
      invitations = v.my_role === 'admin' ? await api.invitations(wsId) : [];
      error = undefined;
      loadError = undefined;
    } catch (e: unknown) {
      loadError = errorText(e, m.members_load_failed());
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
    notice = undefined;
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
    if (done !== undefined) notice = done;
    await load();
    onchanged();
    return true;
  }

  // 다른 워크스페이스로 바뀌면 다시 읽는다.
  $effect(() => {
    void load(workspace.id);
  });
</script>

<section class="flex flex-col gap-8" aria-labelledby="members-heading">
  <h1 id="members-heading" class="text-xl font-semibold">
    {m.members_heading({ workspace: workspace.name })}
  </h1>

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
  <!-- 완료 알림은 처음부터 있는 알림 영역 안에서 바뀐다(새로 만든 영역은 낭독기가 읽지 않을 수 있다). -->
  <p role="status" class="text-sm text-muted-foreground empty:sr-only">{notice ?? ''}</p>

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
        <form
          class="flex flex-wrap items-end gap-2"
          onsubmit={(e) => {
            e.preventDefault();
            if (inviting) return;
            const email = inviteEmail;
            inviteError = undefined;
            inviting = true;
            void act(
              () => api.invite(workspace.id, email, inviteRole),
              m.members_invited({ email }),
              (text: string) => (inviteError = text),
            )
              .then((ok: boolean) => {
                // 성공했을 때만, 그사이 고쳐 치지 않았으면 비운다.
                if (ok && inviteEmail === email) inviteEmail = '';
              })
              .finally(() => (inviting = false));
          }}
        >
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
        <form
          class="flex flex-wrap items-end gap-2"
          onsubmit={(e) => {
            e.preventDefault();
            const name = groupName;
            groupError = undefined;
            void act(
              () => api.createGroup(workspace.id, name),
              undefined,
              (text: string) => (groupError = text),
            ).then((ok: boolean) => {
              if (ok && groupName === name) groupName = '';
            });
          }}
        >
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
          <Button type="submit" disabled={groupName.trim() === ''}
            >{m.members_create_group()}</Button
          >
          {#if groupError !== undefined}
            <p id="group-name-error" role="alert" class="w-full text-sm text-destructive">
              {groupError}
            </p>
          {/if}
        </form>
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
