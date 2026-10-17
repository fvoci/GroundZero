<script lang="ts">
  // 항목 기록(D-33 산출물·위험·문제·회고, D-34 문헌·연구 기록·데이터·변수·분석·투고, D-36 자료·모임·시험·복습,
  // D-38 재료·장비·장비 사용·활동·결과물). 종류와 칸은 서버의 정의대로 그린다. 권한은 항목을 따른다.
  import type { ModuleKind } from '@fvoci/api-client';

  import type { FileView, ItemView, RecordKindView, RecordView } from '$lib/api/client';
  import { api, ApiError, fileContentUrl, newRequestId } from '$lib/api/client';
  import { Badge } from '$lib/ui/badge';
  import { Button } from '$lib/ui/button';

  import RecordEditor from './RecordEditor.svelte';
  import type { RecordForm } from './records';
  import { emptyForm, formOf, summary, toInput } from './records';

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
            modules.filter((m) => m.kind === 'course').map((m) => api.items(m.id)),
          );
          courseList = lists.flat().map((i) => ({ id: i.id, name: i.title }));
        }
        if (cancelled) return;
        defs = mine;
        records = list;
        files = attached;
        people = members.members.map((m) => ({ id: m.user_id, name: m.display_name }));
        courses = courseList;
        if (selected === undefined || !mine.some((d) => d.kind === selected))
          selected = mine[0]?.kind;
      })
      .catch((e: unknown) => {
        if (!cancelled) error = e instanceof ApiError ? e.message : '기록을 불러오지 못했습니다.';
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
      error = '제목을 적어 주세요.';
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
        status = `${def.label}을(를) 더했습니다.`;
      } else if (editingId !== undefined) {
        const r = records.find((x) => x.id === editingId);
        if (r === undefined) return;
        await api.updateRecord(r.id, r.version, built.input);
        status = `${def.label}을(를) 고쳤습니다.`;
      }
      creating = false;
      editingId = undefined;
      reloadKey += 1;
    } catch (e: unknown) {
      error =
        e instanceof ApiError && e.status === 409
          ? '그사이 바뀌었습니다. 다시 불러온 뒤 고쳐 주세요.'
          : e instanceof ApiError
            ? e.message
            : '저장하지 못했습니다.';
    }
  }

  async function remove(r: RecordView): Promise<void> {
    confirming = undefined;
    try {
      await api.deleteRecord(r.id);
      status = '기록을 지웠습니다.';
      reloadKey += 1;
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '지우지 못했습니다.';
    }
  }

  const roleLabel = (def: RecordKindView, role: string): string =>
    def.file_roles.find((r) => r.value === role)?.label ?? role;
</script>

{#if defs.length > 0}
  <section class="flex flex-col gap-3" aria-labelledby="records-heading">
    <h2 id="records-heading" class="text-lg font-semibold">기록</h2>
    <div class="flex flex-wrap gap-1" role="group" aria-label="기록 종류">
      {#each defs as d (d.kind)}
        <Button
          size="sm"
          variant={selected === d.kind ? 'default' : 'outline'}
          aria-pressed={selected === d.kind}
          onclick={() => {
            selected = d.kind;
            creating = false;
            editingId = undefined;
          }}>{d.label} {count(d.kind)}</Button
        >
      {/each}
    </div>
    {#if error !== undefined}<p role="alert" class="text-sm text-destructive">{error}</p>{/if}
    <p aria-live="polite" class="text-sm">{status}</p>
    {#if current !== undefined}
      {@const def = current}
      {#if creating}
        <RecordEditor
          {def}
          bind:form
          {people}
          {courses}
          {files}
          submitLabel={`${def.label} 더하기`}
          onsubmit={() => void save(def)}
          oncancel={() => {
            creating = false;
            editingId = undefined;
          }}
        />
      {:else if canWrite}
        <div><Button size="sm" variant="outline" onclick={startNew}>새 {def.label}</Button></div>
      {/if}
      {#if shown.length === 0}
        <p class="text-sm text-muted-foreground">{def.label} 기록이 없습니다.</p>
      {:else if shown.length > 1}
        <div>
          <Button
            size="sm"
            variant="ghost"
            aria-pressed={asTable}
            onclick={() => (asTable = !asTable)}>{asTable ? '카드로 보기' : '표로 비교하기'}</Button
          >
        </div>
      {/if}
      {#if asTable && shown.length > 1}
        <div class="overflow-x-auto">
          <table class="w-full text-sm" data-testid="records-table">
            <thead class="text-left text-muted-foreground">
              <tr>
                <th class="py-1 pr-2 font-medium">제목</th>
                {#if def.date_label !== null}<th class="pr-2 font-medium">{def.date_label}</th>{/if}
                {#each def.fields as f (f.key)}<th class="pr-2 font-medium">{f.label}</th>{/each}
              </tr>
            </thead>
            <tbody>
              {#each shown as r (r.id)}
                {@const lines = summary(def, r, names, courseNames)}
                <tr class="border-t align-top">
                  <td class="py-1 pr-2 font-medium">{r.title}</td>
                  {#if def.date_label !== null}<td class="pr-2">{r.occurred_on ?? ''}</td>{/if}
                  {#each def.fields as f (f.key)}
                    {@const text = lines.find((l) => l.label === f.label)?.text ?? ''}
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
                submitLabel="저장"
                onsubmit={() => void save(def)}
                oncancel={() => {
                  creating = false;
                  editingId = undefined;
                }}
              />
            {:else}
              <div class="flex flex-wrap items-baseline gap-2">
                <span class="font-medium">{r.title}</span>
                {#if r.occurred_on !== null && def.date_label !== null}
                  <span class="text-muted-foreground">{def.date_label} {r.occurred_on}</span>
                {/if}
                <span class="ml-auto text-xs text-muted-foreground"
                  >{r.created_by_name ?? '이름을 볼 수 없는 사람'}</span
                >
              </div>
              <dl class="grid grid-cols-[max-content_1fr] gap-x-3">
                {#each summary(def, r, names, courseNames) as line (line.label)}
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
                <ul class="flex flex-col gap-0.5" aria-label="연결한 파일">
                  {#each r.files as file, i (i)}
                    <li class="flex flex-wrap items-baseline gap-2">
                      <Badge variant="outline">{roleLabel(def, file.role)}</Badge>
                      {#if file.available && file.file_id !== null}
                        <a class="underline" href={fileContentUrl(file.file_id, file.version)}
                          >{file.file_name}</a
                        >
                      {:else}
                        <span>{file.file_name}</span>
                        <span class="text-muted-foreground">(삭제됨)</span>
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
                    }}>고치기</Button
                  >
                {/if}
                {#if r.can_delete && confirming !== r.id}
                  <Button size="sm" variant="ghost" onclick={() => (confirming = r.id)}
                    >지우기</Button
                  >
                {:else if r.can_delete}
                  <span class="flex items-center gap-1">
                    이 기록을 지울까요?
                    <Button size="sm" variant="destructive" onclick={() => void remove(r)}
                      >지우기</Button
                    >
                    <Button size="sm" variant="ghost" onclick={() => (confirming = undefined)}
                      >취소</Button
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
