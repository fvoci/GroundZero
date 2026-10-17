<script lang="ts">
  // 항목 기록(D-33 산출물·위험·문제·회고, D-34 문헌·연구 기록·데이터·변수·분석·투고, D-36 자료·모임·시험·복습,
  // D-38 재료·장비·장비 사용·활동·결과물). 종류와 칸은 서버의 정의대로 그린다. 권한은 항목을 따른다.
  import type { ModuleKind } from '@fvoci/api-client';

  import type { FileView, ItemView, RecordKindView, RecordView } from '$lib/api/client';
  import { api, ApiError, fileContentUrl, newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { formatDay } from '$lib/display';
  import { m } from '$lib/paraglide/messages.js';
  import { Badge } from '$lib/ui/badge';
  import { Button } from '$lib/ui/button';

  import { recordDateLabel, recordFieldLabel, recordKindLabel, recordRoleLabel } from './labels';
  import RecordEditor from './RecordEditor.svelte';
  import type { RecordForm } from './records';
  import { emptyForm, formOf, summary, toInput } from './records';
  import ZoteroImport from './ZoteroImport.svelte';

  interface Props {
    item: ItemView;
    kind: ModuleKind;
    canWrite: boolean;
  }

  const { item, kind, canWrite }: Props = $props();

  let defs = $state<RecordKindView[]>([]);
  let records = $state<RecordView[]>([]);
  let files = $state<FileView[]>([]);
  let people = $state<{ id: string; name: string }[]>([]);
  let courses = $state<{ id: string; name: string }[]>([]);
  let selected = $state<string | undefined>(undefined);
  let error = $state<string | undefined>(undefined);
  let status = $state('');
  let creating = $state(false);
  let editingId = $state<string | undefined>(undefined);
  let confirming = $state<string | undefined>(undefined);
  // 같은 종류 기록을 표로 나란히 비교(D-34 선행연구 비교 등).
  let asTable = $state(false);
  let form = $state<RecordForm>(emptyForm());
  let requestId = newRequestId();
  let reloadKey = $state(0);

  const current = $derived(defs.find((d) => d.kind === selected));
  const names = $derived(new Map(people.map((p) => [p.id, p.name])));
  const courseNames = $derived(new Map(courses.map((c) => [c.id, c.name])));
  const shown = $derived(records.filter((r) => r.kind === selected));
  const count = (k: string): number => records.filter((r) => r.kind === k).length;

  $effect(() => {
    const id = item.id;
    const ws = item.workspace_id;
    const moduleKind = kind;
    if (reloadKey < 0) return;
    let cancelled = false;
    Promise.all([api.recordKinds(), api.itemRecords(id), api.itemFiles(id), api.members(ws)])
      .then(async ([all, list, attached, members]) => {
        const mine = all.filter((d) => d.module_kind === moduleKind);
        let courseList: { id: string; name: string }[] = [];
        if (mine.some((d) => d.fields.some((f) => f.type === 'course'))) {
          const modules = await api.modules(ws);
          const lists = await Promise.all(
            modules.filter((mod) => mod.kind === 'course').map((mod) => api.items(mod.id)),
          );
          courseList = lists.flat().map((i) => ({ id: i.id, name: i.title }));
        }
        if (cancelled) return;
        defs = mine;
        records = list;
        files = attached;
        people = members.members.map((x) => ({ id: x.user_id, name: x.display_name }));
        courses = courseList;
        if (selected === undefined || !mine.some((d) => d.kind === selected))
          selected = mine[0]?.kind;
      })
      .catch((e: unknown) => {
        if (!cancelled) error = errorText(e, m.records_load_failed());
      });
    return () => {
      cancelled = true;
    };
  });

  function startNew(): void {
    form = emptyForm();
    requestId = newRequestId();
    editingId = undefined;
    creating = true;
  }

  function startEdit(def: RecordKindView, r: RecordView): void {
    form = formOf(def, r);
    creating = false;
    editingId = r.id;
  }

  async function save(def: RecordKindView): Promise<void> {
    if (form.title.trim() === '') {
      error = m.records_title_required();
      return;
    }
    const built = toInput(def, form);
    if ('error' in built) {
      error = built.error;
      return;
    }
    error = undefined;
    try {
      if (creating) {
        await api.createRecord(item.id, requestId, def.kind, built.input);
        status = m.records_added({ label: recordKindLabel(def) });
      } else if (editingId !== undefined) {
        const r = records.find((x) => x.id === editingId);
        if (r === undefined) return;
        await api.updateRecord(r.id, r.version, built.input);
        status = m.records_updated({ label: recordKindLabel(def) });
      }
      creating = false;
      editingId = undefined;
      reloadKey += 1;
    } catch (e: unknown) {
      error =
        e instanceof ApiError && e.status === 409
          ? m.records_conflict()
          : errorText(e, m.records_save_failed());
    }
  }

  async function remove(r: RecordView): Promise<void> {
    confirming = undefined;
    try {
      await api.deleteRecord(r.id);
      status = m.records_deleted();
      reloadKey += 1;
    } catch (e: unknown) {
      error = errorText(e, m.records_delete_failed());
    }
  }

  const roleLabel = (def: RecordKindView, role: string): string => {
    const r = def.file_roles.find((x) => x.value === role);
    return r === undefined ? role : recordRoleLabel(def, r);
  };
</script>

{#if defs.length > 0}
  <section class="flex flex-col gap-3" aria-labelledby="records-heading">
    <h2 id="records-heading" class="text-lg font-semibold">{m.records_heading()}</h2>
    <div class="flex flex-wrap gap-1" role="group" aria-label={m.records_kinds()}>
      {#each defs as d (d.kind)}
        <Button
          size="sm"
          variant={selected === d.kind ? 'default' : 'outline'}
          aria-pressed={selected === d.kind}
          onclick={() => {
            selected = d.kind;
            creating = false;
            editingId = undefined;
          }}>{recordKindLabel(d)} {count(d.kind)}</Button
        >
      {/each}
    </div>
    {#if error !== undefined}<p role="alert" class="text-sm text-destructive">{error}</p>{/if}
    <p aria-live="polite" class="text-sm">{status}</p>
    {#if current !== undefined}
      {@const def = current}
      {@const kindLabel = recordKindLabel(def)}
      {@const dateLabel = recordDateLabel(def)}
      {#if def.kind === 'reference' && canWrite}
        <ZoteroImport
          itemId={item.id}
          onimported={() => {
            reloadKey += 1;
          }}
        />
      {/if}
      {#if creating}
        <RecordEditor
          {def}
          bind:form
          {people}
          {courses}
          {files}
          submitLabel={m.records_add_kind({ label: kindLabel })}
          onsubmit={() => void save(def)}
          oncancel={() => {
            creating = false;
            editingId = undefined;
          }}
        />
      {:else if canWrite}
        <div>
          <Button size="sm" variant="outline" onclick={startNew}
            >{m.records_new_kind({ label: kindLabel })}</Button
          >
        </div>
      {/if}
      {#if shown.length === 0}
        <p class="text-sm text-muted-foreground">{m.records_empty_kind({ label: kindLabel })}</p>
      {:else if shown.length > 1}
        <div>
          <Button
            size="sm"
            variant="ghost"
            aria-pressed={asTable}
            onclick={() => (asTable = !asTable)}
            >{asTable ? m.records_view_cards() : m.records_view_table()}</Button
          >
        </div>
      {/if}
      {#if asTable && shown.length > 1}
        <div class="overflow-x-auto">
          <table class="w-full text-sm" data-testid="records-table">
            <thead class="text-left text-muted-foreground">
              <tr>
                <th class="py-1 pr-2 font-medium">{m.records_title()}</th>
                {#if dateLabel !== null}<th class="pr-2 font-medium">{dateLabel}</th>{/if}
                {#each def.fields as f (f.key)}<th class="pr-2 font-medium"
                    >{recordFieldLabel(def, f)}</th
                  >{/each}
              </tr>
            </thead>
            <tbody>
              {#each shown as r (r.id)}
                {@const lines = summary(def, r, names, courseNames)}
                <tr class="border-t align-top">
                  <td class="py-1 pr-2 font-medium">{r.title}</td>
                  {#if dateLabel !== null}<td class="pr-2"
                      >{r.occurred_on === null ? '' : formatDay(r.occurred_on)}</td
                    >{/if}
                  {#each def.fields as f (f.key)}
                    {@const text = lines.find((l) => l.key === f.key)?.text ?? ''}
                    <td class="max-w-64 pr-2 whitespace-pre-wrap"
                      >{text.length > 120 ? `${text.slice(0, 120)}…` : text}</td
                    >
                  {/each}
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
      <ul
        class={['flex flex-col gap-2', asTable && shown.length > 1 ? 'hidden' : '']}
        data-testid="records"
      >
        {#each shown as r (r.id)}
          <li class="flex flex-col gap-1 rounded-md border p-2 text-sm">
            {#if editingId === r.id}
              <RecordEditor
                {def}
                bind:form
                {people}
                {courses}
                {files}
                submitLabel={m.common_save()}
                onsubmit={() => void save(def)}
                oncancel={() => {
                  creating = false;
                  editingId = undefined;
                }}
              />
            {:else}
              <div class="flex flex-wrap items-baseline gap-2">
                <span class="font-medium">{r.title}</span>
                {#if r.occurred_on !== null && dateLabel !== null}
                  <span class="text-muted-foreground"
                    >{m.records_dated({
                      label: dateLabel,
                      date: formatDay(r.occurred_on),
                    })}</span
                  >
                {/if}
                <span class="ml-auto text-xs text-muted-foreground"
                  >{r.created_by_name ?? m.records_unknown_author()}</span
                >
              </div>
              <dl class="grid grid-cols-[max-content_1fr] gap-x-3">
                {#each summary(def, r, names, courseNames) as line (line.key)}
                  <dt class="text-muted-foreground">{line.label}</dt>
                  <dd class="whitespace-pre-wrap">
                    {#if line.href !== undefined}
                      <a
                        class="underline"
                        href={line.href}
                        target="_blank"
                        rel="noopener noreferrer">{line.text}</a
                      >
                    {:else}{line.text}{/if}
                  </dd>
                {/each}
              </dl>
              {#if r.body !== ''}<p class="whitespace-pre-wrap">{r.body}</p>{/if}
              {#if r.files.length > 0}
                <ul class="flex flex-col gap-0.5" aria-label={m.records_linked_files()}>
                  {#each r.files as file, i (i)}
                    <li class="flex flex-wrap items-baseline gap-2">
                      <Badge variant="outline">{roleLabel(def, file.role)}</Badge>
                      {#if file.available && file.file_id !== null}
                        <a class="underline" href={fileContentUrl(file.file_id, file.version)}
                          >{file.file_name}</a
                        >
                      {:else}
                        <span>{file.file_name}</span>
                        <span class="text-muted-foreground">{m.records_file_deleted()}</span>
                      {/if}
                      <span class="text-muted-foreground"
                        >v{file.version} · SHA-256 {file.sha256.slice(0, 12)}</span
                      >
                    </li>
                  {/each}
                </ul>
              {/if}
              <div class="flex flex-wrap gap-1">
                {#if r.can_edit}
                  <Button
                    size="sm"
                    variant="ghost"
                    onclick={() => {
                      startEdit(def, r);
                    }}>{m.records_edit()}</Button
                  >
                {/if}
                {#if r.can_delete && confirming !== r.id}
                  <Button size="sm" variant="ghost" onclick={() => (confirming = r.id)}
                    >{m.records_delete()}</Button
                  >
                {:else if r.can_delete}
                  <span class="flex items-center gap-1">
                    {m.records_delete_confirm()}
                    <Button size="sm" variant="destructive" onclick={() => void remove(r)}
                      >{m.records_delete()}</Button
                    >
                    <Button size="sm" variant="ghost" onclick={() => (confirming = undefined)}
                      >{m.common_cancel()}</Button
                    >
                  </span>
                {/if}
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{/if}
