<script lang="ts">
  import type { TaskDetailView, TaskView, TaskLinkView, LinkKind } from './model';
  import { LINK_KINDS } from './model';
  import type { LinkPending } from './detail-state';
  import { LINK_KIND_LABEL, LINK_KIND_MEANING, linkText } from './labels';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Label } from '$lib/ui/label';
  import { Input } from '$lib/ui/input';
  import * as Select from '$lib/ui/select';
  import ShiftProposals from './ShiftProposals.svelte';
  let {
    current,
    canEdit,
    busy,
    byId,
    candidates,
    successors,
    label,
    onopen,
    startEdit,
    removeLink,
    applyLink,
    resetLinkEditor,
    decideLink,
    focusLater,
    editing = $bindable(),
    predecessorChoice = $bindable(''),
    linkKind = $bindable('FS'),
    lagText = $bindable('0'),
    linkPending = $bindable(),
  }: {
    current: TaskDetailView;
    canEdit: boolean;
    busy: boolean;
    byId: Map<string, TaskView>;
    candidates: TaskView[];
    successors: { task: TaskView; link: TaskLinkView }[];
    label: (task: TaskView | undefined) => string;
    onopen: (id: string) => void;
    startEdit: (id: string) => void;
    removeLink: (id: string) => Promise<void>;
    applyLink: (id: string) => Promise<void>;
    resetLinkEditor: () => void;
    decideLink: (shifts: boolean) => Promise<void>;
    focusLater: (id: string) => Promise<void>;
    editing: string | undefined;
    predecessorChoice?: string;
    linkKind?: LinkKind;
    lagText?: string;
    linkPending: LinkPending | undefined;
  } = $props();
</script>

<section class="flex flex-col gap-2" aria-labelledby="predecessors-heading">
  <h3 id="predecessors-heading" class="text-sm font-semibold" tabindex="-1">
    {m.tasks_predecessors()}
  </h3>
  {#each current.predecessors as link (link.task_id)}
    {@const pid = link.task_id}
    <div class="flex flex-wrap items-center gap-x-2 text-sm" data-predecessor={pid}>
      <button
        type="button"
        class="rounded px-2 py-1 text-left hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        onclick={() => {
          onopen(pid);
        }}>{label(byId.get(pid)) || m.tasks_hidden_task()}</button
      >
      <span class="text-muted-foreground" data-link-text>{linkText(link)}</span>
      {#if canEdit}
        <span class="ml-auto flex gap-1">
          <Button
            size="sm"
            variant="ghost"
            disabled={busy || linkPending !== undefined}
            aria-label={m.tasks_link_edit_aria({ task: label(byId.get(pid)) })}
            onclick={() => {
              startEdit(pid);
            }}>{m.tasks_link_edit()}</Button
          >
          <Button
            size="sm"
            variant="ghost"
            disabled={busy || linkPending !== undefined}
            aria-label={m.tasks_predecessor_remove_aria({ task: label(byId.get(pid)) })}
            onclick={() => void removeLink(pid)}>{m.tasks_remove()}</Button
          >
        </span>
      {/if}
    </div>
  {:else}
    <p class="text-sm text-muted-foreground">{m.tasks_none()}</p>
  {/each}
  {#if canEdit && (editing !== undefined || candidates.length > 0)}
    <!-- 저장 중에는 칸 전체를 잠근다. 성공 뒤 resetLinkEditor()가 그사이 친 값을 지우지 않게 한다(FA-1, D-97).
                 bits-ui Select는 조상 fieldset의 disabled를 보지 않아 자기 disabled로 잠근다. -->
    <fieldset
      class="flex flex-col gap-2 rounded-md border p-2"
      inert={linkPending !== undefined}
      disabled={busy}
      aria-describedby="link-kind-meaning"
    >
      <legend class="px-1 text-sm font-medium" id="predecessor-label">
        {editing === undefined ? m.tasks_predecessor_add() : m.tasks_link_edit()}
      </legend>
      {#if editing !== undefined}
        <p class="text-sm">{m.tasks_link_editing({ task: label(byId.get(editing)) })}</p>
      {:else}
        <Select.Root
          type="single"
          disabled={busy}
          value={predecessorChoice}
          onValueChange={(v: string) => (predecessorChoice = v)}
        >
          <Select.Trigger aria-labelledby="predecessor-label"
            >{predecessorChoice === ''
              ? m.tasks_predecessor_choose()
              : label(byId.get(predecessorChoice))}</Select.Trigger
          >
          <Select.Content>
            {#each candidates as c (c.id)}
              <Select.Item value={c.id} label={label(c)}>{label(c)}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      {/if}
      <div class="flex flex-wrap items-end gap-2">
        <div class="flex min-w-0 flex-col gap-1">
          <span id="link-kind-label" class="text-sm">{m.tasks_link_kind_label()}</span>
          <Select.Root
            type="single"
            disabled={busy}
            value={linkKind}
            onValueChange={(v: string) => {
              const found = LINK_KINDS.find((k) => k === v);
              if (found !== undefined) linkKind = found;
            }}
          >
            <Select.Trigger aria-labelledby="link-kind-label" data-testid="link-kind"
              >{LINK_KIND_LABEL[linkKind]}</Select.Trigger
            >
            <Select.Content>
              {#each LINK_KINDS as k (k)}
                <Select.Item value={k} label={LINK_KIND_LABEL[k]}>{LINK_KIND_LABEL[k]}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </div>
        <div class="flex w-28 flex-col gap-1">
          <Label for="link-lag">{m.tasks_link_lag_label()}</Label>
          <Input
            id="link-lag"
            inputmode="numeric"
            aria-describedby="link-lag-hint"
            bind:value={lagText}
          />
        </div>
      </div>
      <p id="link-kind-meaning" class="text-sm text-muted-foreground">
        {LINK_KIND_MEANING[linkKind]}
      </p>
      <p id="link-lag-hint" class="text-xs text-muted-foreground">
        {m.tasks_link_lag_hint()}
      </p>
      <div class="flex flex-wrap gap-2">
        <Button
          size="sm"
          disabled={busy || (editing === undefined && predecessorChoice === '')}
          id="link-save"
          onclick={() => void applyLink(editing ?? predecessorChoice)}>{m.tasks_link_save()}</Button
        >
        {#if editing !== undefined}
          <Button size="sm" variant="ghost" disabled={busy} onclick={resetLinkEditor}
            >{m.common_cancel()}</Button
          >
        {/if}
      </div>
    </fieldset>
  {/if}
  {#if linkPending !== undefined}
    <ShiftProposals
      proposals={linkPending.proposals}
      undated={linkPending.undated}
      blocked={linkPending.blocked}
      retried={linkPending.retried}
      {busy}
      forLink
      headingId="link-shift-heading"
      ondecide={(withShifts: boolean) => void decideLink(withShifts)}
      oncancel={() => {
        // 편집 칸은 그대로 두고 관계 저장 단추로 돌린다(고친 값으로 다시 시도할 수 있게).
        linkPending = undefined;
        void focusLater('link-save');
      }}
    />
  {/if}
</section>

<section class="flex flex-col gap-2" aria-labelledby="successors-heading">
  <h3 id="successors-heading" class="text-sm font-semibold">
    {m.tasks_successors_heading()}
  </h3>
  {#each successors as s (s.task.id)}
    <div class="flex flex-wrap items-center gap-x-2 text-sm" data-successor={s.task.id}>
      <button
        type="button"
        class="rounded px-2 py-1 text-left hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        onclick={() => {
          onopen(s.task.id);
        }}>{label(s.task)}</button
      >
      <span class="text-muted-foreground" data-link-text>{linkText(s.link)}</span>
    </div>
  {:else}
    <p class="text-sm text-muted-foreground">{m.tasks_none()}</p>
  {/each}
</section>
