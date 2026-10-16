<script lang="ts">
  // 멤버·초대·그룹·소유권 화면 (D-20, D-51). 권한 판단은 서버가 하고, 화면은 가능한 동작만 보여준다.
  import type { Group, Invitation, MembersView, Role, Workspace } from '@fvoci/api-client';

  import { api, ApiError } from '$lib/api/client';
  import { router } from '$lib/router.svelte';
  import { Badge } from '$lib/ui/badge';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  import RoleSelect from './RoleSelect.svelte';
  import { ROLE_LABEL } from './roles';

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
  let confirm = $state<{ title: string; body: string; run: () => Promise<void> } | undefined>(
    undefined,
  );

  const isAdmin = $derived(view?.my_role === 'admin');

  function nameOf(userId: string): string {
    return view?.members.find((m) => m.user_id === userId)?.display_name ?? userId;
  }

  async function load(wsId: string = workspace.id): Promise<void> {
    try {
      const v = await api.members(wsId);
      view = v;
      groups = await api.groups(wsId);
      invitations = v.my_role === 'admin' ? await api.invitations(wsId) : [];
      error = undefined;
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '멤버 정보를 불러오지 못했습니다.';
    }
  }

  async function act(action: () => Promise<unknown>, done?: string): Promise<void> {
    notice = undefined;
    try {
      await action();
      if (done !== undefined) notice = done;
      await load();
      onchanged();
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '요청을 처리하지 못했습니다.';
    }
  }

  // 다른 워크스페이스로 바뀌면 다시 읽는다.
  $effect(() => {
    void load(workspace.id);
  });
</script>

<section class="flex flex-col gap-8" aria-labelledby="members-heading">
  <h1 id="members-heading" class="text-xl font-semibold">{workspace.name} · 멤버</h1>

  {#if error !== undefined}
    <p role="alert" class="rounded border border-destructive/40 bg-destructive/10 px-3 py-2">
      {error}
    </p>
  {/if}
  {#if notice !== undefined}
    <p role="status" class="text-sm text-muted-foreground">{notice}</p>
  {/if}

  {#if view === undefined}
    <p aria-live="polite">불러오는 중…</p>
  {:else}
    {@const v = view}
    <ul class="flex flex-col divide-y rounded border" data-testid="member-list">
      {#each v.members as member (member.user_id)}
        <li class="flex flex-wrap items-center gap-3 px-3 py-2">
          <span class="font-medium">{member.display_name}</span>
          {#if member.is_owner}<Badge variant="secondary">소유자</Badge>{/if}
          {#if v.pending_owner === member.user_id}<Badge variant="outline">인계 대기</Badge>{/if}
          {#if member.email !== null}
            <span class="text-sm text-muted-foreground">{member.email}</span>
          {/if}
          <span class="ml-auto flex items-center gap-2">
            {#if isAdmin && !member.is_owner}
              <RoleSelect
                value={member.role}
                label={`${member.display_name} 역할`}
                onchange={(role: Role) =>
                  void act(
                    () => api.setRole(workspace.id, member.user_id, role),
                    `${member.display_name}: ${ROLE_LABEL[role]}`,
                  )}
              />
              <Button
                size="sm"
                variant="ghost"
                onclick={() =>
                  (confirm = {
                    title: `${member.display_name}을(를) 제거할까요?`,
                    body: '이 사람이 만든 팀 문서는 워크스페이스에 남습니다.',
                    run: () => act(() => api.removeMember(workspace.id, member.user_id)),
                  })}>제거</Button
              >
              {#if v.i_am_owner}
                <Button
                  size="sm"
                  variant="ghost"
                  onclick={() =>
                    (confirm = {
                      title: `${member.display_name}에게 소유권을 넘길까요?`,
                      body: '상대가 수락해야 완료됩니다. 수락 전에는 취소할 수 있습니다.',
                      run: () => act(() => api.offerOwnership(workspace.id, member.user_id)),
                    })}>소유권 인계</Button
                >
              {/if}
            {:else}
              <span class="text-sm">{ROLE_LABEL[member.role]}</span>
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
        aria-label="소유권 인계"
      >
        <span>{nameOf(pendingOwner)}에게 소유권 인계를 제안했습니다.</span>
        {#if v.i_am_owner}
          <Button
            size="sm"
            variant="ghost"
            onclick={() => void act(() => api.cancelOwnershipOffer(workspace.id))}>제안 취소</Button
          >
        {/if}
        {#if pendingOwner === meId}
          <Button
            size="sm"
            onclick={() =>
              void act(() => api.acceptOwnership(workspace.id), '소유자가 되었습니다.')}
            >소유권 받기</Button
          >
        {/if}
      </div>
    {/if}

    {#if isAdmin}
      <section class="flex flex-col gap-3" aria-labelledby="invite-heading">
        <h2 id="invite-heading" class="font-semibold">초대</h2>
        <p class="text-sm text-muted-foreground">
          초대받은 사람은 로그인한 뒤 같은 이메일로 확인된 계정에서만 수락할 수 있습니다.
        </p>
        <form
          class="flex flex-wrap items-end gap-2"
          onsubmit={(e) => {
            e.preventDefault();
            const email = inviteEmail;
            void act(() => api.invite(workspace.id, email, inviteRole), `${email} 초대함`).then(
              () => {
                inviteEmail = '';
              },
            );
          }}
        >
          <div class="flex min-w-60 flex-1 flex-col gap-1">
            <Label for="invite-email">이메일</Label>
            <Input
              id="invite-email"
              type="email"
              bind:value={inviteEmail}
              autocomplete="off"
              required
            />
          </div>
          <RoleSelect
            value={inviteRole}
            label="초대 역할"
            onchange={(r: Role) => (inviteRole = r)}
          />
          <Button type="submit" disabled={inviteEmail.trim() === ''}>초대</Button>
        </form>
        {#if invitations.length > 0}
          <ul class="flex flex-col gap-1" data-testid="invitation-list">
            {#each invitations as inv (inv.id)}
              <li class="flex items-center gap-2 text-sm">
                <span>{inv.email}</span>
                <Badge variant="outline">{ROLE_LABEL[inv.role]}</Badge>
                <span class="text-muted-foreground"
                  >~{new Date(inv.expires_at).toLocaleDateString()}</span
                >
                <Button
                  size="sm"
                  variant="ghost"
                  onclick={() => void act(() => api.revokeInvitation(inv.id))}>취소</Button
                >
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}

    <section class="flex flex-col gap-3" aria-labelledby="groups-heading">
      <h2 id="groups-heading" class="font-semibold">그룹</h2>
      {#if groups.length === 0}
        <p class="text-sm text-muted-foreground">그룹이 없습니다.</p>
      {/if}
      {#each groups as group (group.id)}
        <div class="flex flex-col gap-2 rounded border px-3 py-2">
          <div class="flex items-center gap-2">
            <span class="font-medium">{group.name}</span>
            <span class="text-sm text-muted-foreground">{group.members.length}명</span>
            {#if isAdmin}
              <Button
                size="sm"
                variant="ghost"
                class="ml-auto"
                onclick={() => void act(() => api.deleteGroup(group.id))}>그룹 삭제</Button
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
          class="flex items-end gap-2"
          onsubmit={(e) => {
            e.preventDefault();
            const name = groupName;
            void act(() => api.createGroup(workspace.id, name)).then(() => {
              groupName = '';
            });
          }}
        >
          <div class="flex flex-1 flex-col gap-1">
            <Label for="group-name">새 그룹 이름</Label>
            <Input id="group-name" bind:value={groupName} maxlength={200} autocomplete="off" />
          </div>
          <Button type="submit" disabled={groupName.trim() === ''}>그룹 만들기</Button>
        </form>
      {/if}
    </section>

    {#if !v.i_am_owner}
      <section class="flex flex-col gap-2" aria-labelledby="leave-heading">
        <h2 id="leave-heading" class="font-semibold">탈퇴</h2>
        <div>
          <Button
            variant="outline"
            onclick={() =>
              (confirm = {
                title: `${workspace.name}에서 탈퇴할까요?`,
                body: '내가 만든 팀 문서는 워크스페이스에 남습니다. 다시 들어오려면 초대가 필요합니다.',
                run: async () => {
                  await api.leave(workspace.id);
                  onchanged();
                  router.navigate('/');
                },
              })}>워크스페이스 탈퇴</Button
          >
        </div>
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
      <Button variant="ghost" onclick={() => (confirm = undefined)}>취소</Button>
      <Button
        onclick={() => {
          const run = confirm?.run;
          confirm = undefined;
          if (run !== undefined)
            void run().catch((e: unknown) => {
              error = e instanceof ApiError ? e.message : '요청을 처리하지 못했습니다.';
            });
        }}>확인</Button
      >
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
