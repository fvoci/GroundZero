<script lang="ts">
  // 항목 정보(D-33·D-34·D-36·D-38): 상태·기간·리더·목표·범위와 종류별 정보, 비공개 항목의 멤버(사람·그룹).
  // 멤버는 비공개 항목을 워크스페이스 역할 안에서 볼 수 있는 사람이다(D-20 공유). 정하는 것은 항목 소유 권한자.
  import type { ModuleKind } from '@fvoci/api-client';

  import type { AssigneeView, ItemProfileView, ItemStatus, ItemView } from '$lib/api/client';
  import { api, ApiError, ITEM_STATUSES } from '$lib/api/client';
  import { Badge } from '$lib/ui/badge';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import * as Select from '$lib/ui/select';

  import { detailFields, ITEM_STATUS_LABEL, peopleLabels, toDetails } from './profile';

  interface Props {
    item: ItemView;
    kind: ModuleKind;
    /** 상태·기간이 바뀌면 항목을 다시 읽는다. */
    onchanged: () => void;
  }

  const { item, kind, onchanged }: Props = $props();

  let profile = $state<ItemProfileView | undefined>(undefined);
  let people = $state<AssigneeView[]>([]);
  let error = $state<string | undefined>(undefined);
  let status = $state('');
  let editing = $state(false);
  let managing = $state(false);
  let reloadKey = $state(0);
  interface FormState {
    status: ItemStatus;
    leader: string;
    goal: string;
    scope: string;
    starts_on: string;
    ends_on: string;
    keywords: string;
    values: Record<string, string>;
  }

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

  const fields = $derived(detailFields(kind));
  const labels = $derived(peopleLabels(kind));
  const textarea =
    'rounded-md border border-input bg-transparent px-3 py-2 text-base shadow-xs focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none';

  $effect(() => {
    const id = item.id;
    if (reloadKey < 0) return;
    let cancelled = false;
    api
      .itemProfile(id)
      .then((p) => {
        if (!cancelled) profile = p;
      })
      .catch((e: unknown) => {
        if (!cancelled)
          error = e instanceof ApiError ? e.message : '항목 정보를 불러오지 못했습니다.';
      });
    return () => {
      cancelled = true;
    };
  });

  async function startEdit(p: ItemProfileView): Promise<void> {
    form = {
      status: p.status,
      leader: p.leader?.user_id ?? '',
      goal: p.goal,
      scope: p.scope,
      starts_on: p.starts_on ?? '',
      ends_on: p.ends_on ?? '',
      keywords: (p.details.keywords ?? []).join(', '),
      values: Object.fromEntries(fields.map((f) => [f.key, p.details[f.key] ?? ''])),
    };
    editing = true;
    people = await api.assignable(item.id).catch(() => []);
  }

  async function save(e: SubmitEvent): Promise<void> {
    e.preventDefault();
    if (form.starts_on !== '' && form.ends_on !== '' && form.starts_on > form.ends_on) {
      error = '기간의 끝이 시작보다 앞섭니다.';
      return;
    }
    error = undefined;
    try {
      await api.setItemProfile(item.id, {
        leader_id: form.leader === '' ? null : form.leader,
        goal: form.goal,
        scope: form.scope,
        starts_on: form.starts_on === '' ? null : form.starts_on,
        ends_on: form.ends_on === '' ? null : form.ends_on,
        status: form.status,
        details: toDetails(kind, form.values, form.keywords),
      });
      editing = false;
      status = '항목 정보를 저장했습니다.';
      reloadKey += 1;
      onchanged();
    } catch (err: unknown) {
      error = err instanceof ApiError ? err.message : '저장하지 못했습니다.';
    }
  }

  async function startMembers(p: ItemProfileView): Promise<void> {
    error = undefined;
    try {
      const [view, groups] = await Promise.all([
        api.members(item.workspace_id),
        api.groups(item.workspace_id),
      ]);
      candidates = view.members.map((m) => ({ id: m.user_id, name: m.display_name }));
      groupChoices = groups.map((g) => ({ id: g.id, name: g.name }));
      memberIds = p.members.map((m) => m.user_id);
      groupIds = p.groups.map((g) => g.id);
      managing = true;
    } catch (err: unknown) {
      error = err instanceof ApiError ? err.message : '멤버 목록을 불러오지 못했습니다.';
    }
  }

  function toggle(list: string[], id: string, on: boolean): string[] {
    const rest = list.filter((x) => x !== id);
    return on ? [...rest, id] : rest;
  }

  async function saveMembers(): Promise<void> {
    error = undefined;
    try {
      await api.setItemMembers(item.id, memberIds, groupIds);
      managing = false;
      status = `${labels.members}을(를) 저장했습니다.`;
      reloadKey += 1;
    } catch (err: unknown) {
      error = err instanceof ApiError ? err.message : '저장하지 못했습니다.';
    }
  }
</script>

<section class="flex flex-col gap-3" aria-labelledby="item-profile-heading">
  <div class="flex items-center gap-2">
    <h2 id="item-profile-heading" class="text-lg font-semibold">정보</h2>
    {#if profile !== undefined && profile.can_edit && !editing}
      {@const p = profile}
      <Button size="sm" variant="outline" onclick={() => void startEdit(p)}>고치기</Button>
    {/if}
  </div>
  {#if error !== undefined}<p role="alert" class="text-sm text-destructive">{error}</p>{/if}
  <p aria-live="polite" class="text-sm">{status}</p>
  {#if profile === undefined}
    <p class="text-sm text-muted-foreground">불러오는 중…</p>
  {:else if editing}
    <form class="grid gap-3 sm:grid-cols-2" onsubmit={(e) => void save(e)} novalidate>
      <div class="flex flex-col gap-1">
        <span id="item-status-label" class="text-sm font-medium">상태</span>
        <Select.Root
          type="single"
          value={form.status}
          onValueChange={(v: string) => {
            form.status = ITEM_STATUSES.find((s) => s === v) ?? form.status;
          }}
        >
          <Select.Trigger class="w-32" aria-labelledby="item-status-label"
            >{ITEM_STATUS_LABEL[form.status]}</Select.Trigger
          >
          <Select.Content>
            {#each ITEM_STATUSES as s (s)}
              <Select.Item value={s} label={ITEM_STATUS_LABEL[s]}
                >{ITEM_STATUS_LABEL[s]}</Select.Item
              >
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
      <div class="flex flex-col gap-1">
        <span id="item-leader-label" class="text-sm font-medium">{labels.leader}</span>
        <Select.Root
          type="single"
          value={form.leader === '' ? 'none' : form.leader}
          onValueChange={(v: string) => {
            form.leader = v === 'none' ? '' : v;
          }}
        >
          <Select.Trigger class="w-48" aria-labelledby="item-leader-label"
            >{people.find((p) => p.user_id === form.leader)?.display_name ??
              profile.leader?.display_name ??
              '없음'}</Select.Trigger
          >
          <Select.Content>
            <Select.Item value="none" label="없음">없음</Select.Item>
            {#each people as p (p.user_id)}
              <Select.Item value={p.user_id} label={p.display_name}>{p.display_name}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
      <div class="flex flex-col gap-1">
        <Label for="item-starts">시작일</Label>
        <Input id="item-starts" type="date" bind:value={form.starts_on} />
      </div>
      <div class="flex flex-col gap-1">
        <Label for="item-ends">종료일</Label>
        <Input id="item-ends" type="date" bind:value={form.ends_on} />
      </div>
      <div class="flex flex-col gap-1 sm:col-span-2">
        <Label for="item-goal">목표</Label>
        <textarea id="item-goal" class={textarea} rows={2} maxlength={4000} bind:value={form.goal}
        ></textarea>
      </div>
      <div class="flex flex-col gap-1 sm:col-span-2">
        <Label for="item-scope">범위</Label>
        <textarea id="item-scope" class={textarea} rows={2} maxlength={4000} bind:value={form.scope}
        ></textarea>
      </div>
      {#if kind === 'research'}
        <div class="flex flex-col gap-1 sm:col-span-2">
          <Label for="item-keywords">키워드(쉼표로 여럿)</Label>
          <Input id="item-keywords" bind:value={form.keywords} />
        </div>
      {/if}
      {#each fields as f (f.key)}
        <div class={['flex flex-col gap-1', f.multiline ? 'sm:col-span-2' : '']}>
          <Label for={`item-detail-${f.key}`}>{f.label}</Label>
          {#if f.multiline}
            <textarea
              id={`item-detail-${f.key}`}
              class={textarea}
              rows={2}
              maxlength={2000}
              bind:value={form.values[f.key]}></textarea>
          {:else}
            <Input id={`item-detail-${f.key}`} maxlength={2000} bind:value={form.values[f.key]} />
          {/if}
        </div>
      {/each}
      <div class="flex gap-2 sm:col-span-2">
        <Button type="submit">저장</Button>
        <Button type="button" variant="ghost" onclick={() => (editing = false)}>취소</Button>
      </div>
    </form>
  {:else}
    {@const p = profile}
    <dl class="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1 text-sm">
      <dt class="text-muted-foreground">상태</dt>
      <dd><Badge variant="outline">{ITEM_STATUS_LABEL[p.status]}</Badge></dd>
      {#if p.starts_on !== null || p.ends_on !== null}
        <dt class="text-muted-foreground">기간</dt>
        <dd>{p.starts_on ?? ''} ~ {p.ends_on ?? ''}</dd>
      {/if}
      <dt class="text-muted-foreground">{labels.leader}</dt>
      <dd>{p.leader?.display_name ?? '없음'}</dd>
      {#if p.goal !== ''}
        <dt class="text-muted-foreground">목표</dt>
        <dd class="whitespace-pre-wrap">{p.goal}</dd>
      {/if}
      {#if p.scope !== ''}
        <dt class="text-muted-foreground">범위</dt>
        <dd class="whitespace-pre-wrap">{p.scope}</dd>
      {/if}
      {#if (p.details.keywords ?? []).length > 0}
        <dt class="text-muted-foreground">키워드</dt>
        <dd>{(p.details.keywords ?? []).join(', ')}</dd>
      {/if}
      {#each fields as f (f.key)}
        {#if p.details[f.key] !== undefined}
          <dt class="text-muted-foreground">{f.label}</dt>
          <dd class="whitespace-pre-wrap">{p.details[f.key]}</dd>
        {/if}
      {/each}
    </dl>
  {/if}

  {#if profile !== undefined}
    {@const p = profile}
    <div class="flex flex-col gap-2 text-sm">
      <div class="flex items-center gap-2">
        <span class="font-medium">{labels.members}</span>
        {#if p.can_manage_members && !managing}
          <Button size="sm" variant="outline" onclick={() => void startMembers(p)}>정하기</Button>
        {/if}
      </div>
      <p class="text-muted-foreground">
        {item.visibility === 'private'
          ? '비공개 항목은 만든 사람·관리자와 여기의 사람·그룹만 봅니다(작성자는 고치고 독자는 읽기).'
          : '워크스페이스에 공개된 항목이라 멤버 모두가 봅니다. 비공개로 바꾸면 여기의 사람·그룹만 봅니다.'}
      </p>
      {#if managing}
        <fieldset class="flex flex-col gap-2 rounded-md border p-2">
          <legend class="px-1">사람</legend>
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
          <fieldset class="flex flex-col gap-2 rounded-md border p-2">
            <legend class="px-1">그룹</legend>
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
          <Button size="sm" onclick={() => void saveMembers()}>저장</Button>
          <Button size="sm" variant="ghost" onclick={() => (managing = false)}>취소</Button>
        </div>
      {:else if p.members.length === 0 && p.groups.length === 0}
        <p>없음</p>
      {:else}
        <ul class="flex flex-wrap gap-1" aria-label={labels.members}>
          {#each p.members as m (m.user_id)}<li>
              <Badge variant="secondary">{m.display_name}</Badge>
            </li>{/each}
          {#each p.groups as g (g.id)}<li>
              <Badge variant="outline">그룹 · {g.name}</Badge>
            </li>{/each}
        </ul>
      {/if}
    </div>
  {/if}
</section>
