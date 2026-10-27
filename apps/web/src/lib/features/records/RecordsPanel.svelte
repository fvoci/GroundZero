<script lang="ts">
  // 항목 기록(D-33 산출물·위험·문제·회고, D-34 문헌·연구 기록·데이터·변수·분석·투고, D-36 자료·모임·시험·복습,
  // D-38 재료·장비·장비 사용·활동·결과물). 종류와 칸은 서버의 정의대로 그린다. 권한은 항목을 따른다.
  import type { ModuleKind } from '@fvoci/api-client';
  import { onMount, tick } from 'svelte';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { object, retry } from '../modules/draft-values';

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
  import { emptyForm, formOf, parseRecordForm, summary, toInput } from './records';
  import ZoteroImport from './ZoteroImport.svelte';

  interface Props {
    item: ItemView;
    kind: ModuleKind;
    canWrite: boolean;
    /** 이 기록 종류만 보인다(항목 화면의 첫 묶음: 연구 문헌, 취미 활동, D-121). 없으면 모든 종류. */
    only?: readonly string[] | undefined;
    /** 이 기록 종류는 빼고 보인다(첫 묶음에 간 종류를 공통 기록 묶음에서 뺀다). 없으면 모든 종류. */
    except?: readonly string[] | undefined;
  }

  const { item, kind, canWrite, only, except }: Props = $props();

  let defs = $state<RecordKindView[]>([]);
  let records = $state<RecordView[]>([]);
  let files = $state<FileView[]>([]);
  let people = $state<{ id: string; name: string }[]>([]);
  let courses = $state<{ id: string; name: string }[]>([]);
  let episodes = $state<{ id: string; name: string }[]>([]);
  let selected = $state<string | undefined>(undefined);
  let error = $state<string | undefined>(undefined);
  /** 기록 종류·기록 목록을 읽지 못했다(저장 오류와 따로). 처음부터 못 읽으면 종류가 없어도 이 안내는 보인다. */
  let loadError = $state<string | undefined>(undefined);
  /** 회차 목록(D-123)만 못 읽었다. 기록은 보이고 회차 이름·선택이 빈다. */
  let episodeError = $state<string | undefined>(undefined);
  let status = $state('');
  let creating = $state(false);
  let editingId = $state<string | undefined>(undefined);
  /**
   * 고치는 기록의 기준 판: 고치기를 연 때 읽은 판. 저장 때의 최신 목록에서 가져오면 고치는 사이 목록을 다시 읽은 것만으로
   * 서버의 판 확인이 꺼져 다른 사람의 고침을 덮는다. 409 뒤에도 그대로 두고, 충돌 안내에서 서버 내용 불러오기를 골랐을
   * 때만 그 판으로 옮긴다(덮어쓰기는 그 저장 한 번만 보여 준 판을 보낸다).
   */
  let editingVersion = $state(0);
  /** 409 뒤 다시 읽은, 다른 사람이 먼저 고친 그 기록의 지금 내용. 고르기 전에는 입력과 기준 판을 그대로 둔다. */
  let conflict = $state<RecordView | undefined>(undefined);
  let confirming = $state<string | undefined>(undefined);
  // 같은 종류 기록을 표로 나란히 비교(D-34 선행연구 비교 등).
  let asTable = $state(false);
  let form = $state<RecordForm>(emptyForm());
  // 같은 입력의 재시도는 같은 요청 id, 고친 입력은 새 id(처음 것이 돌아와 고친 내용이 사라지지 않게).
  let pending = $state<{ requestId: string; signature: string } | null>(null);
  let baseline = $state(JSON.stringify(emptyForm()));
  let busy = $state(false);
  const draft = createFormDraft({
    name: () =>
      selected === undefined ? undefined : `record:${item.id}:${selected}:${editingId ?? 'new'}`,
    snapshot: () => ({ form, baseline, version: editingVersion, pending }),
    parse: (raw) => {
      if (
        !object(raw) ||
        typeof raw.baseline !== 'string' ||
        typeof raw.version !== 'number' ||
        !retry(raw.pending)
      )
        return undefined;
      const parsed = parseRecordForm(raw.form);
      return parsed === undefined
        ? undefined
        : { form: parsed, baseline: raw.baseline, version: raw.version, pending: raw.pending };
    },
    restore: (value) => {
      form = value.form;
      baseline = value.baseline;
      editingVersion = value.version;
      pending = value.pending;
      creating = editingId === undefined;
    },
    reset: () => {
      form = emptyForm();
      baseline = JSON.stringify(form);
      pending = null;
      busy = false;
      conflict = undefined;
    },
    dirty: () => (creating || editingId !== undefined) && JSON.stringify(form) !== baseline,
    busy: () => busy,
  });
  onMount(draft.attach);
  let reloadKey = $state(0);

  const current = $derived(defs.find((d) => d.kind === selected));
  const names = $derived(new Map(people.map((p) => [p.id, p.name])));
  const courseNames = $derived(new Map(courses.map((c) => [c.id, c.name])));
  const episodeNames = $derived(new Map(episodes.map((e) => [e.id, e.name])));
  const shown = $derived(records.filter((r) => r.kind === selected));

  // 기록에 이을 첨부 파일 목록은 기록을 쓰거나 고칠 때만 받는다(첨부 패널과 같은 목록을 화면을 열 때마다 두 번 받지 않는다).
  const formOpen = $derived(creating || editingId !== undefined);
  $effect(() => {
    if (!formOpen) return;
    const id = item.id;
    let cancelled = false;
    api.itemFiles(id).then(
      (list) => {
        if (!cancelled) files = list;
      },
      () => {
        // 못 받으면 파일 잇기만 비워 둔다(기록 쓰기는 그대로 된다).
        if (!cancelled) files = [];
      },
    );
    return () => {
      cancelled = true;
    };
  });
  const count = (k: string): number => records.filter((r) => r.kind === k).length;

  $effect(() => {
    const id = item.id;
    const ws = item.workspace_id;
    const moduleKind = kind;
    const shownKinds = only;
    const hiddenKinds = except;
    if (reloadKey < 0) return;
    let cancelled = false;
    Promise.all([api.recordKinds(), api.itemRecords(id), api.members(ws)])
      .then(async ([all, list, members]) => {
        const mine = all.filter(
          (d) =>
            d.module_kind === moduleKind &&
            (shownKinds === undefined || shownKinds.includes(d.kind)) &&
            !(hiddenKinds?.includes(d.kind) ?? false),
        );
        let courseList: { id: string; name: string }[] = [];
        if (mine.some((d) => d.fields.some((f) => f.type === 'course'))) {
          const modules = await api.modules(ws);
          const lists = await Promise.all(
            modules.filter((mod) => mod.kind === 'course').map((mod) => api.items(mod.id)),
          );
          courseList = lists.flat().map((i) => ({ id: i.id, name: i.title }));
        }
        // 회차 칸(D-123 강의 질문·복습)은 이 강좌의 회차 가운데 고른다. 번호와 제목으로 보인다. 회차를 못 읽어도 기록 목록은
        // 보이고, 회차 이름·선택만 따로 실패를 알린다(CodeRabbit).
        let episodeList: { id: string; name: string }[] = [];
        let episodeFailure: string | undefined;
        if (mine.some((d) => d.fields.some((f) => f.type === 'episode'))) {
          try {
            const course = await api.course(id);
            episodeList = course.episodes.map((e) => ({
              id: e.id,
              name: `${String(e.number)}. ${e.title}`,
            }));
          } catch (e: unknown) {
            episodeFailure = errorText(e, m.records_episodes_load_failed());
          }
        }
        if (cancelled) return;
        loadError = undefined;
        episodeError = episodeFailure;
        defs = mine;
        records = list;
        // 고치던 기록이 그사이 지워졌으면 편집기가 말없이 사라지지 않게 알린다.
        if (editingId !== undefined && !list.some((r) => r.id === editingId)) {
          editingId = undefined;
          error = m.common_error_not_found();
        }
        people = members.members.map((x) => ({ id: x.user_id, name: x.display_name }));
        courses = courseList;
        episodes = episodeList;
        if (selected === undefined || !mine.some((d) => d.kind === selected))
          selected = mine[0]?.kind;
      })
      .catch((e: unknown) => {
        if (!cancelled) loadError = errorText(e, m.records_load_failed());
      });
    return () => {
      cancelled = true;
    };
  });

  async function startNew(): Promise<void> {
    editingId = undefined;
    await tick();
    form = emptyForm();
    baseline = JSON.stringify(form);
    pending = null;
    conflict = undefined;
    creating = true;
  }

  async function startEdit(def: RecordKindView, r: RecordView): Promise<void> {
    creating = false;
    editingId = r.id;
    await tick();
    form = formOf(def, r);
    baseline = JSON.stringify(form);
    editingVersion = r.version;
    conflict = undefined;
  }

  /**
   * 409를 받은 뒤 이 항목의 기록을 다시 읽는다. 다른 사람이 먼저 고쳐 판이 바뀌었으면(보관으로 막힌 409와 구분) 입력과
   * 기준 판은 그대로 두고 지금 저장된 내용을 보여 준다. 다시 저장해도 같은 기준 판을 보내 덮어쓰지 않고, 덮어쓰기를
   * 골라야 바꾼다. 기록이 지워졌으면 고치기를 닫고 알린다. 처리했으면 true.
   */
  async function recordMovedOn(
    id: string,
    version: number,
    stillCurrent: () => boolean,
  ): Promise<boolean> {
    let list: RecordView[];
    try {
      list = await api.itemRecords(item.id);
    } catch {
      return false;
    }
    if (!stillCurrent()) return true;
    records = list;
    // 기다리는 사이 고치기를 닫았거나 다른 기록을 열었으면 그 편집에 손대지 않는다.
    if (editingId !== id) return true;
    const latest = list.find((x) => x.id === id);
    if (latest === undefined) {
      editingId = undefined;
      error = m.common_error_not_found();
      return true;
    }
    if (latest.version === version) return false;
    conflict = latest;
    return true;
  }

  /**
   * `baseVersion`은 고치기에서 보낼 기준 판. 충돌 안내에서 덮어쓰기를 고르면 보여 준 서버 판을 보낸다(그사이 또 바뀌면
   * 다시 409로 알린다). 입력이 틀려 보내지 못하거나 실패하면 기준 판과 충돌 안내는 그대로 남는다.
   */
  async function save(def: RecordKindView, baseVersion = editingVersion): Promise<void> {
    if (busy) return;
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
    const base =
      !creating && editingId !== undefined ? { id: editingId, version: baseVersion } : undefined;
    if (creating) {
      const signature = JSON.stringify({ item: item.id, kind: def.kind, input: built.input });
      if (pending?.signature !== signature) pending = { requestId: newRequestId(), signature };
    }
    const attempt = draft.capture();
    if (attempt === undefined) return;
    busy = true;
    try {
      let savedStatus: string;
      if (creating && pending !== null) {
        const requestId = pending.requestId;
        await api.createRecord(item.id, requestId, def.kind, built.input);
        savedStatus = m.records_added({ label: recordKindLabel(def) });
      } else if (base !== undefined) {
        await api.updateRecord(base.id, base.version, built.input);
        savedStatus = m.records_updated({ label: recordKindLabel(def) });
      } else return;
      if (!draft.complete(attempt)) return;
      status = savedStatus;
      creating = false;
      editingId = undefined;
      conflict = undefined;
      reloadKey += 1;
    } catch (e: unknown) {
      if (!draft.isCurrent(attempt)) return;
      const refused = e instanceof ApiError && e.status === 409;
      if (
        refused &&
        base !== undefined &&
        (await recordMovedOn(base.id, base.version, () => draft.isCurrent(attempt)))
      )
        return;
      if (!draft.isCurrent(attempt)) return;
      error = refused ? m.records_conflict() : errorText(e, m.records_save_failed());
    } finally {
      if (draft.isOwner(attempt)) busy = false;
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

<!-- 저장하는 동안 되살리기도 잠근다. 그사이 바뀐 입력은 성공한 저장을 버리게 하고, 만들기는 다시 내면 새 요청 id로
     두 번째 기록을, 고치기는 옮기지 못한 기준 판으로 자기 저장과 409를 낸다. -->
{#each draft.offers as entry (entry.key)}
  <fieldset class="contents" disabled={busy}>
    <FormDraftOffer
      onrestore={() => {
        draft.restore(entry);
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

<!-- 종류를 처음부터 못 읽었으면(종류가 없는 정상 빈 상태와 달리) 제목과 함께 실패를 알리고 다시 읽을 수 있게 한다. -->
{#if defs.length > 0 || loadError !== undefined}
  <section class="flex flex-col gap-3" aria-labelledby="records-heading">
    <h2 id="records-heading" class="text-lg font-semibold">{m.records_heading()}</h2>
    <div class="flex flex-wrap gap-1" role="group" aria-label={m.records_kinds()}>
      {#each defs as d (d.kind)}
        <Button
          size="sm"
          variant={selected === d.kind ? 'default' : 'outline'}
          aria-pressed={selected === d.kind}
          onclick={() => {
            draft.leave.request(() => {
              selected = d.kind;
              creating = false;
              editingId = undefined;
              conflict = undefined;
            });
          }}>{recordKindLabel(d)} {count(d.kind)}</Button
        >
      {/each}
    </div>
    {#if loadError !== undefined}
      <div class="flex flex-wrap items-center gap-2">
        <p role="alert" class="text-sm text-destructive">{loadError}</p>
        <Button
          size="sm"
          variant="outline"
          onclick={() => {
            reloadKey += 1;
          }}>{m.common_retry()}</Button
        >
      </div>
    {/if}
    {#if episodeError !== undefined}
      <div class="flex flex-wrap items-center gap-2">
        <p role="alert" class="text-sm text-destructive">{episodeError}</p>
        <Button
          size="sm"
          variant="outline"
          onclick={() => {
            reloadKey += 1;
          }}>{m.common_retry()}</Button
        >
      </div>
    {/if}
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
          {episodes}
          {files}
          disabled={busy}
          submitLabel={m.records_add_kind({ label: kindLabel })}
          onsubmit={() => void save(def)}
          oncancel={() => {
            draft.leave.request(() => {
              creating = false;
              editingId = undefined;
            });
          }}
        />
      {:else if canWrite}
        <div>
          <Button
            size="sm"
            variant="outline"
            onclick={() => {
              draft.leave.request(() => void startNew());
            }}>{m.records_new_kind({ label: kindLabel })}</Button
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
                {@const lines = summary(def, r, names, courseNames, episodeNames)}
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
              {#if conflict?.id === r.id}
                {@const latest = conflict}
                <!-- 다른 사람이 먼저 고쳤다: 지금 저장된 내용을 보이고, 불러올지 덮어쓸지 사용자가 고른다(일정 충돌과 같은 원칙). -->
                <div
                  class="flex flex-col gap-2 rounded-md border border-amber-500/50 p-3"
                  role="alert"
                >
                  <p>{m.records_changed()}</p>
                  <dl class="grid grid-cols-[max-content_1fr] gap-x-3">
                    <dt class="text-muted-foreground">{m.records_title()}</dt>
                    <dd>{latest.title}</dd>
                    {#if latest.occurred_on !== null && dateLabel !== null}
                      <dt class="text-muted-foreground">{dateLabel}</dt>
                      <dd>{formatDay(latest.occurred_on)}</dd>
                    {/if}
                    {#each summary(def, latest, names, courseNames, episodeNames) as line (line.key)}
                      <dt class="text-muted-foreground">{line.label}</dt>
                      <dd class="whitespace-pre-wrap">{line.text}</dd>
                    {/each}
                    {#if latest.body !== ''}
                      <dt class="text-muted-foreground">{m.records_body()}</dt>
                      <dd class="whitespace-pre-wrap">{latest.body}</dd>
                    {/if}
                    {#if latest.files.length > 0}
                      <dt class="text-muted-foreground">{m.records_linked_files()}</dt>
                      <dd>
                        {latest.files.map((f) => `${f.file_name} v${String(f.version)}`).join(', ')}
                      </dd>
                    {/if}
                  </dl>
                  <div class="flex flex-wrap gap-2">
                    <Button
                      size="sm"
                      variant="outline"
                      onclick={async () => {
                        await startEdit(def, latest);
                        // 안내가 사라지며 누른 단추도 없어지므로 초점을 제목 칸으로 옮긴다(키보드 사용자가 자리를 잃지 않게).
                        await tick();
                        document.getElementById(`record-${def.kind}-title`)?.focus();
                      }}>{m.records_load_server()}</Button
                    >
                    <Button size="sm" onclick={() => void save(def, latest.version)}
                      >{m.records_overwrite()}</Button
                    >
                  </div>
                </div>
              {/if}
              <RecordEditor
                {def}
                bind:form
                {people}
                {courses}
                {episodes}
                {files}
                disabled={busy}
                submitLabel={m.common_save()}
                onsubmit={() => void save(def)}
                oncancel={() => {
                  draft.leave.request(() => {
                    creating = false;
                    editingId = undefined;
                    conflict = undefined;
                  });
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
                {#each summary(def, r, names, courseNames, episodeNames) as line (line.key)}
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
                      draft.leave.request(() => void startEdit(def, r));
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
