<script lang="ts">
  import { taskData as work } from '$lib/features/tasks/detail-data';
  // 항목 정보(D-33·D-34·D-36·D-38): 상태·기간·리더·목표·범위와 종류별 정보, 비공개 항목의 멤버(사람·그룹).
  // 멤버는 비공개 항목을 워크스페이스 역할 안에서 볼 수 있는 사람이다(D-20 공유). 정하는 것은 항목 소유 권한자.
  import type { ModuleKind } from '@fvoci/api-client';
  import { onMount, tick, untrack } from 'svelte';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import FieldConflict from '$lib/features/common/FieldConflict.svelte';
  import { selectedFields } from '$lib/features/common/field-conflict';
  import { readTaskSessionGeneration } from '$lib/features/tasks/context-read';
  import { me } from '$lib/me.svelte';
  import { object, strings } from './draft-values';
  import {
    itemProfileFields,
    nextComparison,
    parseComparison,
    parseProfileInput,
    profileInputOf,
    type ItemComparison,
    type ItemProfileInput,
  } from './item-conflict';

  import type { AssigneeView, ItemProfileView, ItemView, PersonRefView } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
  import { ITEM_STATUSES } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import {
    accountSafety,
    accountSwitching,
    notifyDone,
  } from '$lib/features/feedback/feedback.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { Badge } from '$lib/ui/badge';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import { Textarea } from '$lib/ui/textarea';
  import * as Select from '$lib/ui/select';

  import ItemProfileSummary from './ItemProfileSummary.svelte';
  import {
    detailFields,
    itemStatusLabel,
    peopleLabels,
    toDetails,
    type ItemProfileRead,
  } from './profile';

  interface Props {
    item: ItemView;
    kind: ModuleKind;
    /** 상태·기간이 바뀌면 항목을 다시 읽는다. */
    onchanged: () => void;
    /** 앱 안 이동의 다음 경로에서도 이 패널이 마운트된 채인지(항목 화면 머리, D-121). 그러면 묻지 않는다. */
    stays?: ((next: string) => boolean) | undefined;
    onprofile?: (read: ItemProfileRead) => void;
    refresh?: number;
    ondirty?: (dirty: boolean) => void;
    onrestore?: () => void;
  }

  const {
    item,
    kind,
    onchanged,
    stays,
    onprofile,
    refresh = 0,
    ondirty,
    onrestore,
  }: Props = $props();
  let profile = $state<ItemProfileView | undefined>(undefined);
  let profileRead = $state<ItemProfileRead | undefined>(undefined);
  let people = $state<AssigneeView[]>([]);
  let error = $state<string | undefined>(undefined);
  let editing = $state(false);
  // 편집을 연 때의 판(D-104). 편집 중에 패널을 다시 읽어도 옮기지 않는다(옮기면 그사이 다른 사람이 고친 것을 덮는다).
  let editBase = $state(0);
  // 편집을 열거나 취소할 때마다 오른다. 저장을 기다리는 동안 취소하고 다시 연 편집을 앞선 저장이 닫지 않게 한다.
  let editSession = 0;
  // 편집을 연(또는 초안을 되살린) 사용자. 저장의 기대 사용자 머리로 보낸다(D-96, #307).
  let editUser = '';
  // 편집을 연 때의 리더, 그 뒤 고른 리더. 고를 수 있는 사람 목록에 없어도(못 읽음·되살린 초안) 입력의 리더 이름을
  // 보이도록 초안에도 남긴다.
  let editLeader = $state<PersonRefView | null>(null);
  // 판 충돌(409) 뒤 칸마다 고르는 비교(D-94 개정, D-104). 닫아도 저장하기 전까지 처음 입력을 지키려고 남긴다.
  let comparison = $state<ItemComparison<ItemProfileInput>>();
  // 409가 알려 준 리더 이름(고를 수 있는 사람 목록에 없을 수 있다). 비교의 리더 칸 이름표에 쓴다.
  let leaderNames = $state<Record<string, string>>({});
  // 저장이 권한 상실(403·404)이나 확인된 보관으로 거절되면 다시 보내지 않는다. 404면 비교한 서버 값도 숨긴다.
  let writeBlocked = $state(false);
  let hiddenForAccess = $state(false);
  let managing = $state(false);
  let reloadKey = $state(0);
  type FormState = ItemProfileInput;

  let form = $state<FormState>({
    status: 'active',
    leader: '',
    goal: '',
    scope: '',
    starts_on: '',
    ends_on: '',
    keywords: '',
    values: {},
  });
  let memberIds = $state<string[]>([]);
  let groupIds = $state<string[]>([]);
  let candidates = $state<{ id: string; name: string }[]>([]);
  let groupChoices = $state<{ id: string; name: string }[]>([]);
  let baseline = $state('');
  let busy = $state(false);
  let membersBaseline = $state('');
  let membersBusy = $state(false);
  const memberSnapshot = () => JSON.stringify([[...memberIds].sort(), [...groupIds].sort()]);
  const membersDirty = $derived(managing && memberSnapshot() !== membersBaseline);
  const membersDraft = createFormDraft({
    preserve: false,
    name: () => `item-members:${item.id}`,
    snapshot: () => ({ memberIds, groupIds }),
    parse: () => undefined,
    restore: () => undefined,
    reset: () => {
      managing = false;
      memberIds = [];
      groupIds = [];
      membersBusy = false;
    },
    dirty: () => membersDirty,
    busy: () => membersBusy,
    stays: (next) => stays?.(next) === true,
  });
  onMount(membersDraft.attach);
  const draft = createFormDraft({
    name: () => `item-profile:${item.id}:${kind}`,
    snapshot: () => ({
      form,
      base: editBase,
      baseline,
      leader: editLeader,
      comparison,
      leaders: leaderNames,
    }),
    parse: (raw) => {
      if (
        !object(raw) ||
        !object(raw.form) ||
        typeof raw.base !== 'number' ||
        typeof raw.baseline !== 'string'
      )
        return undefined;
      // 리더 이름이 없는 초안(이전 형식)은 이름 없이 되살린다.
      const leader =
        object(raw.leader) &&
        typeof raw.leader.user_id === 'string' &&
        typeof raw.leader.display_name === 'string'
          ? { user_id: raw.leader.user_id, display_name: raw.leader.display_name }
          : null;
      const value = raw.form;
      const status = ITEM_STATUSES.find((candidate) => candidate === value.status);
      if (
        status === undefined ||
        typeof value.leader !== 'string' ||
        typeof value.goal !== 'string' ||
        typeof value.scope !== 'string' ||
        typeof value.starts_on !== 'string' ||
        typeof value.ends_on !== 'string' ||
        typeof value.keywords !== 'string' ||
        !strings(value.values)
      )
        return undefined;
      return {
        form: {
          status,
          leader: value.leader,
          goal: value.goal,
          scope: value.scope,
          starts_on: value.starts_on,
          ends_on: value.ends_on,
          keywords: value.keywords,
          values: value.values,
        },
        base: raw.base,
        baseline: raw.baseline,
        leader,
        // 다른 항목의 비교는 되살리지 않는다.
        comparison: parseComparison(raw.comparison, item.id, parseProfileInput),
        leaders: strings(raw.leaders) ? raw.leaders : {},
      };
    },
    restore: (value) => {
      form = value.form;
      editBase = value.base;
      baseline = value.baseline;
      editLeader = value.leader;
      comparison = value.comparison;
      leaderNames = value.leaders;
      editing = true;
      editSession += 1;
      editUser = untrack(() => me.userId);
      // 편집 열기(startEdit)처럼 고를 수 있는 사람을 읽는다. 그사이 편집을 닫거나 새로 열었으면 버린다.
      const session = editSession;
      void work
        .assignable(item.id)
        .then((list) => {
          if (session === editSession) people = list;
        })
        .catch(() => undefined);
    },
    reset: () => {
      // 비교를 먼저 비운다. 버린 비교가 초안에 다시 쓰이면 다음에 되살리자고 한다(CR-305-2).
      comparison = undefined;
      leaderNames = {};
      editing = false;
      busy = false;
      editSession += 1;
      form = {
        status: 'active',
        leader: '',
        goal: '',
        scope: '',
        starts_on: '',
        ends_on: '',
        keywords: '',
        values: {},
      };
      baseline = '';
    },
    dirty: () => editing && (comparison !== undefined || JSON.stringify(form) !== baseline),
    stays: (next) => stays?.(next) === true,
  });
  onMount(draft.attach);
  $effect(() => {
    if (profileRead !== undefined) onprofile?.(profileRead);
  });
  $effect(() => {
    ondirty?.(
      busy ||
        membersBusy ||
        (editing && (comparison !== undefined || JSON.stringify(form) !== baseline)) ||
        membersDirty,
    );
  });

  // 리더 칸은 입력만 보인다. 서버의 리더(409의 최신본 등)를 대신 보이면 "내 입력으로 다시 저장"이 보이지 않게 리더를 지운다.
  const leaderLabel = $derived.by(() => {
    if (form.leader === '') return m.modules_none();
    const known = [...people, profile?.leader, editLeader].find((p) => p?.user_id === form.leader);
    return known?.display_name ?? m.modules_leader_unknown();
  });
  const fields = $derived(detailFields(kind));
  const labels = $derived(peopleLabels(kind));

  function acceptProfile(id: string, next: ItemProfileView): void {
    profile = next;
    profileRead = { status: 'ready', itemId: id, profile: next };
  }

  $effect(() => {
    const id = item.id;
    if (reloadKey + refresh < 0) return;
    let cancelled = false;
    untrack(() => {
      // 같은 항목의 재조회 중에는 편집 폼을 유지하고, 요약만 로딩으로 바꾼다.
      if (profileRead?.itemId !== id) profile = undefined;
      profileRead = { status: 'loading', itemId: id };
      error = undefined;
    });
    api
      .itemProfile(id)
      .then((p) => {
        if (!cancelled) acceptProfile(id, p);
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        error = errorText(e, m.modules_profile_load_failed());
        profileRead = { status: 'failed', itemId: id, error };
      });
    return () => {
      cancelled = true;
    };
  });

  async function startEdit(p: ItemProfileView): Promise<void> {
    error = undefined;
    editSession += 1;
    editUser = me.userId;
    editBase = p.version;
    editLeader = p.leader;
    comparison = undefined;
    leaderNames = {};
    writeBlocked = false;
    hiddenForAccess = false;
    form = profileInputOf(p, kind);
    baseline = JSON.stringify(form);
    editing = true;
    people = await work.assignable(item.id).catch(() => []);
  }

  async function save(e: SubmitEvent): Promise<void> {
    e.preventDefault();
    await submit(editBase);
  }

  /** 입력을 `base` 판에 대한 고치기로 보낸다(충돌 알림의 "내 입력으로 다시 저장"은 알림이 보인 최신본의 판). */
  async function submit(base: number): Promise<void> {
    if (busy || writeBlocked) return;
    const attempt = draft.capture();
    if (attempt === undefined) return;
    // 앞선 저장의 안내가 이번 결과(날짜 오류·409·실패) 옆에 남아 저장된 것처럼 보이지 않게 한다.
    if (form.starts_on !== '' && form.ends_on !== '' && form.starts_on > form.ends_on) {
      error = m.modules_period_invalid();
      return;
    }
    error = undefined;
    const session = editSession;
    const captured = writeContext();
    // 같은 편집·같은 사용자·같은 세션 세대의 응답만 화면에 넣는다(D-96, CR-A).
    const owns = (): boolean =>
      session === editSession && draft.isOwner(attempt) && sameWriteContext(captured);
    busy = true;
    try {
      const result = await api.setItemProfile(
        item.id,
        base,
        {
          leader_id: form.leader === '' ? null : form.leader,
          goal: form.goal,
          scope: form.scope,
          starts_on: form.starts_on === '' ? null : form.starts_on,
          ends_on: form.ends_on === '' ? null : form.ends_on,
          status: form.status,
          details: toDetails(kind, form.values, form.keywords),
        },
        editUser,
      );
      if (!draft.isCurrent(attempt)) return;
      if (result.kind === 'conflict') {
        // 기다리는 동안 취소했거나 편집을 새로 열었으면 그 화면을 바꾸지 않는다(이 저장은 아무것도 바꾸지 않았다).
        if (!owns()) return;
        // 그사이 다른 사람이 고쳤다. 서버는 아무것도 바꾸지 않았다. 입력은 그대로 두고 바로 칸마다 고르게 한다.
        openProfileComparison(result.current);
        acceptProfile(item.id, result.current);
        return;
      }
      // 저장은 새 판을 주지 않으므로(204) 다시 읽은 뒤에 편집을 닫는다. 먼저 닫으면 다시 읽기 전에 연 편집이 옛 판을
      // 기준으로 삼아 다음 저장이 거짓 409가 된다.
      const fresh = await api.itemProfile(item.id).catch(() => undefined);
      if (!draft.isOwner(attempt)) return;
      onchanged();
      if (session !== editSession && editing) {
        // 기다리는 동안 취소하고 편집을 새로 열었다. 그 편집은 닫거나 바꾸지 않는다(그 편집의 판이 지났으면 저장이
        // 409로 알린다).
        return;
      }
      if (session === editSession) {
        if (!draft.complete(attempt)) return;
        comparison = undefined;
        editing = false;
        notifyDone(m.modules_profile_saved());
      }
      // 다시 읽지 못하면 정보를 비우고 다시 읽기를 맡긴다. 옛 판이 남아 있으면 그 판으로 편집을 다시 열 수 있다.
      if (fresh === undefined) {
        profile = undefined;
        profileRead = { status: 'loading', itemId: item.id };
        reloadKey += 1;
      } else acceptProfile(item.id, fresh);
    } catch (err: unknown) {
      if (owns()) {
        error = errorText(err, m.modules_save_failed());
        await profileWriteFailed(err, owns);
      }
    } finally {
      if (draft.isOwner(attempt)) busy = false;
    }
  }

  const writeContext = () => ({
    userId: me.userId,
    session: readTaskSessionGeneration(),
    safety: accountSafety.generation,
  });
  function sameWriteContext(captured: ReturnType<typeof writeContext>): boolean {
    const now = writeContext();
    return (
      captured.userId === now.userId &&
      captured.session === now.session &&
      captured.safety === now.safety &&
      !accountSafety.switching &&
      !accountSafety.restoring
    );
  }
  function blockDenied(cause: unknown): void {
    if (!(cause instanceof ApiError)) return;
    if (cause.status === 403 || cause.status === 404) writeBlocked = true;
    if (cause.status === 404) hiddenForAccess = true;
  }
  /** 409 `invalid_state`는 보관과 일시 충돌이 같은 코드라 한 번 다시 읽어 확인될 때만 막는다(SR-302-1). */
  async function profileWriteFailed(cause: unknown, owns: () => boolean): Promise<void> {
    blockDenied(cause);
    if (!(cause instanceof ApiError) || cause.status !== 409 || cause.code !== 'invalid_state')
      return;
    try {
      const fresh = await api.itemProfile(item.id);
      if (owns()) writeBlocked = !fresh.can_edit;
    } catch (failure: unknown) {
      if (owns()) blockDenied(failure);
    }
  }

  const leaderNameMap = $derived(
    new Map<string, string>([
      ...Object.entries(leaderNames),
      ...[editLeader, profile?.leader, ...people].flatMap((p): [string, string][] =>
        p === null || p === undefined ? [] : [[p.user_id, p.display_name]],
      ),
    ]),
  );
  const conflictFields = $derived(itemProfileFields(kind, leaderNameMap, labels.leader));
  const baseInput = (): FormState => {
    try {
      return parseProfileInput(JSON.parse(baseline)) ?? $state.snapshot(form);
    } catch {
      return $state.snapshot(form);
    }
  };
  /** 편집 기준·이번 입력·409의 서버 값을 비교한다. 409의 `changed`는 양쪽 변경 판정이 아니라 쓰지 않는다. */
  function openProfileComparison(current: ItemProfileView): void {
    if (current.leader !== null)
      leaderNames = { ...leaderNames, [current.leader.user_id]: current.leader.display_name };
    comparison = nextComparison($state.snapshot(comparison), conflictFields, {
      target: item.id,
      base: baseInput(),
      mine: $state.snapshot(form),
      server: profileInputOf(current, kind),
      version: current.version,
      baseVersion: editBase,
    });
    if (!current.can_edit) writeBlocked = true;
  }
  async function applyProfileChoices(saveNow: boolean): Promise<void> {
    const ask = comparison;
    const current = profile;
    if (ask === undefined || !ask.open || current === undefined || busy || writeBlocked) return;
    // 되살린 비교가 지금 읽은 판보다 옛 판이면 새로 읽은 판과 다시 비교한다(CR-305-1). 요청은 보내지 않는다.
    if (current.version > ask.version) {
      openProfileComparison($state.snapshot(current));
      return;
    }
    const candidate = selectedFields(
      conflictFields,
      $state.snapshot(ask.mine),
      $state.snapshot(ask.server),
      ask.choices,
    );
    form = candidate;
    baseline = JSON.stringify(ask.server);
    editBase = ask.version;
    if (!saveNow) {
      ask.open = false;
      await tick();
      document.getElementById('item-goal')?.focus();
      return;
    }
    if (conflictFields.every((field) => field.same(candidate, ask.server))) {
      // 고른 값이 모두 서버 최신값이다. 보낼 것이 없다. 그래도 서버의 상태·기간이 바뀌었을 수 있어 항목을 다시
      // 읽게 한다(CR-306-1).
      comparison = undefined;
      editing = false;
      onchanged();
      notifyDone(m.common_conflict_equal());
      return;
    }
    await submit(ask.version);
  }
  function leaveProfileComparison(original: boolean): void {
    const ask = comparison;
    if (ask === undefined || !ask.open || busy) return;
    form = $state.snapshot(original ? ask.original : ask.mine);
    baseline = JSON.stringify(original ? ask.originalBase : ask.base);
    editBase = original ? ask.originalVersion : ask.baseVersion;
    ask.open = false;
    void tick().then(() => document.getElementById('item-goal')?.focus());
  }
  const profileEdited = (a: FormState, b: FormState): boolean =>
    !conflictFields.every((field) => field.same(a, b));

  async function startMembers(p: ItemProfileView): Promise<void> {
    error = undefined;
    try {
      const [view, groups] = await Promise.all([
        api.members(item.workspace_id),
        api.groups(item.workspace_id),
      ]);
      candidates = view.members.map((x) => ({ id: x.user_id, name: x.display_name }));
      groupChoices = groups.map((g) => ({ id: g.id, name: g.name }));
      memberIds = p.members.map((x) => x.user_id);
      groupIds = p.groups.map((g) => g.id);
      membersBaseline = memberSnapshot();
      managing = true;
    } catch (err: unknown) {
      error = errorText(err, m.modules_members_load_failed());
    }
  }

  function toggle(list: string[], id: string, on: boolean): string[] {
    const rest = list.filter((x) => x !== id);
    return on ? [...rest, id] : rest;
  }

  async function saveMembers(): Promise<void> {
    if (membersBusy) return;
    const attempt = membersDraft.capture();
    if (attempt === undefined) return;
    error = undefined;
    membersBusy = true;
    try {
      await api.setItemMembers(item.id, memberIds, groupIds);
      if (!membersDraft.complete(attempt)) return;
      notifyDone(m.modules_people_saved({ label: labels.members }));
      reloadKey += 1;
    } catch (err: unknown) {
      if (membersDraft.isOwner(attempt)) error = errorText(err, m.modules_save_failed());
    } finally {
      if (membersDraft.isOwner(attempt)) membersBusy = false;
    }
  }
</script>

<!-- 저장하는 동안 입력과 되살리기를 잠근다(D-97). 그사이 바뀐 입력은 성공한 저장을 버리게 하고, 기준 판을 옮기지 않은
     다음 저장이 자기 저장과 409로 부딪친다. -->
{#each draft.offers as entry (entry.key)}
  <fieldset class="contents" disabled={busy}>
    <FormDraftOffer
      restorable={profile?.can_edit === true && !accountSwitching()}
      onrestore={() => {
        if (profile?.can_edit === true && !accountSwitching()) {
          draft.restore(entry);
          onrestore?.();
        }
      }}
      ondiscard={() => {
        draft.discard(entry);
      }}
    />
  </fieldset>
{/each}
<DiscardChangesConfirm
  open={draft.leave.open}
  onkeep={draft.leave.keep}
  ondiscard={draft.discardAndLeave}
/>
<DiscardChangesConfirm
  open={membersDraft.leave.open}
  onkeep={membersDraft.leave.keep}
  ondiscard={membersDraft.discardAndLeave}
/>

<section class="flex flex-col gap-3" aria-labelledby="item-profile-heading">
  <div class="flex items-center gap-2">
    <h2 id="item-profile-heading" class="text-lg font-semibold">{m.modules_profile()}</h2>
    {#if profile !== undefined && profile.can_edit && !editing}
      {@const p = profile}
      <Button size="sm" variant="outline" onclick={() => void startEdit(p)}
        >{m.modules_edit()}</Button
      >
    {/if}
  </div>
  {#if error !== undefined}<p role="alert" class="text-sm text-destructive">{error}</p>{/if}
  {#if profile === undefined}
    {#if profileRead?.status !== 'failed'}
      <p class="text-sm text-muted-foreground">{m.modules_loading()}</p>
    {/if}
  {:else if editing}
    <fieldset class="flex min-w-0 flex-col gap-3" disabled={busy}>
      {#if comparison?.open === true && !hiddenForAccess}
        {@const ask = comparison}
        <!-- 판 충돌(D-94 개정, D-104): 칸마다 내 입력·서버 최신을 고른다. 고르는 동안 폼은 잠근다. -->
        <FieldConflict
          fields={conflictFields}
          base={ask.base}
          mine={ask.mine}
          server={ask.server}
          bind:choices={ask.choices}
          {busy}
          blocked={writeBlocked}
          repeated={ask.repeated}
          onsave={() => void applyProfileChoices(true)}
          onedit={() => void applyProfileChoices(false)}
          oncancel={() => {
            leaveProfileComparison(false);
          }}
          onoriginal={profileEdited(ask.original, ask.mine)
            ? () => {
                leaveProfileComparison(true);
              }
            : undefined}
        />
      {/if}
      <!-- 404로 비교를 숨기면 폼을 잠그지 않는다. 취소로 닫을 수 있고 저장은 writeBlocked가 막는다(CR-306-2). -->
      <form
        class="grid gap-3 @xl:grid-cols-2"
        onsubmit={(e) => void save(e)}
        novalidate
        inert={comparison?.open === true && !hiddenForAccess}
      >
        <div class="flex flex-col gap-1">
          <!-- bits-ui Select는 조상 fieldset의 disabled를 보지 않고 열리므로 저장 중에는 따로 잠근다(상태·리더). -->
          <span id="item-status-label" class="text-sm font-medium">{m.modules_status()}</span>
          <Select.Root
            type="single"
            disabled={busy}
            value={form.status}
            onValueChange={(v: string) => {
              form.status = ITEM_STATUSES.find((s) => s === v) ?? form.status;
            }}
          >
            <Select.Trigger class="w-32" aria-labelledby="item-status-label"
              >{itemStatusLabel(form.status)}</Select.Trigger
            >
            <Select.Content>
              {#each ITEM_STATUSES as s (s)}
                <Select.Item value={s} label={itemStatusLabel(s)}>{itemStatusLabel(s)}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </div>
        <div class="flex flex-col gap-1">
          <span id="item-leader-label" class="text-sm font-medium">{labels.leader}</span>
          <Select.Root
            type="single"
            disabled={busy}
            value={form.leader === '' ? 'none' : form.leader}
            onValueChange={(v: string) => {
              form.leader = v === 'none' ? '' : v;
              const picked = people.find((p) => p.user_id === v);
              if (picked !== undefined)
                editLeader = { user_id: picked.user_id, display_name: picked.display_name };
            }}
          >
            <Select.Trigger class="w-48" aria-labelledby="item-leader-label"
              >{leaderLabel}</Select.Trigger
            >
            <Select.Content>
              <Select.Item value="none" label={m.modules_none()}>{m.modules_none()}</Select.Item>
              {#each people as p (p.user_id)}
                <Select.Item value={p.user_id} label={p.display_name}>{p.display_name}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </div>
        <div class="flex flex-col gap-1">
          <Label for="item-starts">{m.modules_starts_on()}</Label>
          <Input id="item-starts" type="date" bind:value={form.starts_on} />
        </div>
        <div class="flex flex-col gap-1">
          <Label for="item-ends">{m.modules_ends_on()}</Label>
          <Input id="item-ends" type="date" bind:value={form.ends_on} />
        </div>
        <div class="flex flex-col gap-1 @xl:col-span-2">
          <Label for="item-goal">{m.modules_goal()}</Label>
          <Textarea id="item-goal" rows={2} maxlength={4000} bind:value={form.goal} />
        </div>
        <div class="flex flex-col gap-1 @xl:col-span-2">
          <Label for="item-scope">{m.modules_scope()}</Label>
          <Textarea id="item-scope" rows={2} maxlength={4000} bind:value={form.scope} />
        </div>
        {#if kind === 'research'}
          <div class="flex flex-col gap-1 @xl:col-span-2">
            <Label for="item-keywords">{m.modules_keywords_input()}</Label>
            <Input id="item-keywords" bind:value={form.keywords} />
          </div>
        {/if}
        {#each fields as f (f.key)}
          <div class={['flex flex-col gap-1', f.multiline ? '@xl:col-span-2' : '']}>
            <Label for={`item-detail-${f.key}`}>{f.label}</Label>
            {#if f.multiline}
              <Textarea
                id={`item-detail-${f.key}`}
                rows={2}
                maxlength={2000}
                bind:value={form.values[f.key]}
              />
            {:else}
              <Input id={`item-detail-${f.key}`} maxlength={2000} bind:value={form.values[f.key]} />
            {/if}
          </div>
        {/each}
        <div class="flex gap-2 @xl:col-span-2">
          <Button type="submit">{m.common_save()}</Button>
          <Button
            type="button"
            variant="ghost"
            onclick={() => {
              draft.leave.request(() => {
                comparison = undefined;
                editSession += 1;
                editing = false;
                busy = false;
                error = undefined;
              });
            }}>{m.common_cancel()}</Button
          >
        </div>
      </form>
    </fieldset>
  {:else}
    <ItemProfileSummary {profile} {kind} />
  {/if}

  {#if profile !== undefined}
    {@const p = profile}
    <div class="flex flex-col gap-2 text-sm">
      <div class="flex items-center gap-2">
        <span class="font-medium">{labels.members}</span>
        {#if p.can_manage_members && !managing}
          <Button size="sm" variant="outline" onclick={() => void startMembers(p)}
            >{m.modules_manage_members()}</Button
          >
        {/if}
      </div>
      <p class="text-muted-foreground">
        {item.visibility === 'private'
          ? m.modules_members_private_hint()
          : m.modules_members_shared_hint()}
      </p>
      {#if managing}
        <fieldset class="flex flex-col gap-2 rounded-md border p-2" disabled={membersBusy}>
          <legend class="px-1">{m.modules_people()}</legend>
          {#each candidates as c (c.id)}
            <div class="flex items-center gap-2">
              <Checkbox
                id={`item-member-${c.id}`}
                checked={memberIds.includes(c.id)}
                onCheckedChange={(on: boolean) => {
                  memberIds = toggle(memberIds, c.id, on);
                }}
              />
              <Label for={`item-member-${c.id}`}>{c.name}</Label>
            </div>
          {/each}
        </fieldset>
        {#if groupChoices.length > 0}
          <fieldset class="flex flex-col gap-2 rounded-md border p-2" disabled={membersBusy}>
            <legend class="px-1">{m.modules_groups()}</legend>
            {#each groupChoices as g (g.id)}
              <div class="flex items-center gap-2">
                <Checkbox
                  id={`item-group-${g.id}`}
                  checked={groupIds.includes(g.id)}
                  onCheckedChange={(on: boolean) => {
                    groupIds = toggle(groupIds, g.id, on);
                  }}
                />
                <Label for={`item-group-${g.id}`}>{g.name}</Label>
              </div>
            {/each}
          </fieldset>
        {/if}
        <div class="flex gap-2">
          <Button size="sm" disabled={membersBusy} onclick={() => void saveMembers()}
            >{m.common_save()}</Button
          >
          <Button
            size="sm"
            variant="ghost"
            disabled={membersBusy}
            onclick={() => {
              membersDraft.leave.request(() => (managing = false));
            }}>{m.common_cancel()}</Button
          >
        </div>
      {:else if p.members.length === 0 && p.groups.length === 0}
        <p>{m.modules_none()}</p>
      {:else}
        <!-- 공통 Badge는 긴 글을 말줄임 없이 자른다. 사람·그룹 이름은 길 수 있어 칸 폭에서 말줄임하고 전체 이름은 title로. -->
        <ul class="flex flex-wrap gap-1" aria-label={labels.members}>
          {#each p.members as member (member.user_id)}<li class="min-w-0 max-w-full">
              <Badge variant="secondary" class="max-w-full" title={member.display_name}
                ><span class="truncate">{member.display_name}</span></Badge
              >
            </li>{/each}
          {#each p.groups as g (g.id)}
            {@const label = m.modules_group_badge({ name: g.name })}
            <li class="min-w-0 max-w-full">
              <Badge variant="outline" class="max-w-full" title={label}
                ><span class="truncate">{label}</span></Badge
              >
            </li>{/each}
        </ul>
      {/if}
    </div>
  {/if}
</section>
