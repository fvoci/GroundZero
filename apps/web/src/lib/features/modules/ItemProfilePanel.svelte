<script lang="ts">
  // 항목 정보(D-33·D-34·D-36·D-38): 상태·기간·리더·목표·범위와 종류별 정보, 비공개 항목의 멤버(사람·그룹).
  // 멤버는 비공개 항목을 워크스페이스 역할 안에서 볼 수 있는 사람이다(D-20 공유). 정하는 것은 항목 소유 권한자.
  import type { ModuleKind } from '@fvoci/api-client';

  import type { AssigneeView, ItemProfileView, ItemStatus, ItemView } from '$lib/api/client';
  import { api, ITEM_STATUSES } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { Badge } from '$lib/ui/badge';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import * as Select from '$lib/ui/select';

  import { detailFields, itemStatusLabel, peopleLabels, periodText, toDetails } from './profile';

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
        if (!cancelled) error = errorText(e, m.modules_profile_load_failed());
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
      error = m.modules_period_invalid();
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
      status = m.modules_profile_saved();
      reloadKey += 1;
      onchanged();
    } catch (err: unknown) {
      error = errorText(err, m.modules_save_failed());
    }
  }

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
    error = undefined;
    try {
      await api.setItemMembers(item.id, memberIds, groupIds);
      managing = false;
      status = m.modules_people_saved({ label: labels.members });
      reloadKey += 1;
    } catch (err: unknown) {
      error = errorText(err, m.modules_save_failed());
    }
  }
</script>

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
  <p aria-live="polite" class="text-sm">{status}</p>
  {#if profile === undefined}
    <p class="text-sm text-muted-foreground">{m.modules_loading()}</p>
  {:else if editing}
    <form class="grid gap-3 sm:grid-cols-2" onsubmit={(e) => void save(e)} novalidate>
      <div class="flex flex-col gap-1">
        <span id="item-status-label" class="text-sm font-medium">{m.modules_status()}</span>
        <Select.Root
          type="single"
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
          value={form.leader === '' ? 'none' : form.leader}
          onValueChange={(v: string) => {
            form.leader = v === 'none' ? '' : v;
          }}
        >
          <Select.Trigger class="w-48" aria-labelledby="item-leader-label"
            >{people.find((p) => p.user_id === form.leader)?.display_name ??
              profile.leader?.display_name ??
              m.modules_none()}</Select.Trigger
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
      <div class="flex flex-col gap-1 sm:col-span-2">
        <Label for="item-goal">{m.modules_goal()}</Label>
        <textarea id="item-goal" class={textarea} rows={2} maxlength={4000} bind:value={form.goal}
        ></textarea>
      </div>
      <div class="flex flex-col gap-1 sm:col-span-2">
        <Label for="item-scope">{m.modules_scope()}</Label>
        <textarea id="item-scope" class={textarea} rows={2} maxlength={4000} bind:value={form.scope}
        ></textarea>
      </div>
      {#if kind === 'research'}
        <div class="flex flex-col gap-1 sm:col-span-2">
          <Label for="item-keywords">{m.modules_keywords_input()}</Label>
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
        <Button type="submit">{m.common_save()}</Button>
        <Button type="button" variant="ghost" onclick={() => (editing = false)}
          >{m.common_cancel()}</Button
        >
      </div>
    </form>
  {:else}
    {@const p = profile}
    <dl class="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1 text-sm">
      <dt class="text-muted-foreground">{m.modules_status()}</dt>
      <dd><Badge variant="outline">{itemStatusLabel(p.status)}</Badge></dd>
      {#if p.starts_on !== null || p.ends_on !== null}
        <dt class="text-muted-foreground">{m.modules_period()}</dt>
        <dd>{periodText(p.starts_on, p.ends_on)}</dd>
      {/if}
      <dt class="text-muted-foreground">{labels.leader}</dt>
      <dd>{p.leader?.display_name ?? m.modules_none()}</dd>
      {#if p.goal !== ''}
        <dt class="text-muted-foreground">{m.modules_goal()}</dt>
        <dd class="whitespace-pre-wrap">{p.goal}</dd>
      {/if}
      {#if p.scope !== ''}
        <dt class="text-muted-foreground">{m.modules_scope()}</dt>
        <dd class="whitespace-pre-wrap">{p.scope}</dd>
      {/if}
      {#if (p.details.keywords ?? []).length > 0}
        <dt class="text-muted-foreground">{m.modules_keywords()}</dt>
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
        <fieldset class="flex flex-col gap-2 rounded-md border p-2">
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
          <fieldset class="flex flex-col gap-2 rounded-md border p-2">
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
          <Button size="sm" onclick={() => void saveMembers()}>{m.common_save()}</Button>
          <Button size="sm" variant="ghost" onclick={() => (managing = false)}
            >{m.common_cancel()}</Button
          >
        </div>
      {:else if p.members.length === 0 && p.groups.length === 0}
        <p>{m.modules_none()}</p>
      {:else}
        <ul class="flex flex-wrap gap-1" aria-label={labels.members}>
          {#each p.members as member (member.user_id)}<li>
              <Badge variant="secondary">{member.display_name}</Badge>
            </li>{/each}
          {#each p.groups as g (g.id)}<li>
              <Badge variant="outline">{m.modules_group_badge({ name: g.name })}</Badge>
            </li>{/each}
        </ul>
      {/if}
    </div>
  {/if}
</section>
