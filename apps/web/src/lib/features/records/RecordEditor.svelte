<script lang="ts">
  // 기록 입력 칸. 칸 정의(서버)대로 그린다. 파일 연결은 이 항목의 첨부와 그 버전을 고른다(D-35).
  import type { FileView, RecordKindView } from '$lib/api/client';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  import type { RecordForm } from './records';
  import { emptyForm } from './records';

  interface Props {
    def: RecordKindView;
    form?: RecordForm;
    people: { id: string; name: string }[];
    courses: { id: string; name: string }[];
    files: FileView[];
    submitLabel: string;
    onsubmit: () => void;
    oncancel: () => void;
  }

  let {
    def,
    form = $bindable(emptyForm()),
    people,
    courses,
    files,
    submitLabel,
    onsubmit,
    oncancel,
  }: Props = $props();

  const box =
    'rounded-md border border-input bg-transparent px-3 py-2 text-base shadow-xs focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none';
  const id = (key: string): string => `record-${def.kind}-${key}`;

  function versionsOf(fileId: string): number[] {
    const f = files.find((x) => x.id === fileId);
    return f === undefined
      ? []
      : Array.from({ length: f.current_version }, (_, i) => f.current_version - i);
  }

  function togglePerson(key: string, person: string, on: boolean): void {
    const rest = (form.people[key] ?? []).filter((p) => p !== person);
    form.people[key] = on ? [...rest, person] : rest;
  }
</script>

<form
  class="grid gap-3 rounded-md border p-3 sm:grid-cols-2"
  novalidate
  onsubmit={(e: SubmitEvent) => {
    e.preventDefault();
    onsubmit();
  }}
>
  <div class="flex flex-col gap-1 sm:col-span-2">
    <Label for={id('title')}>제목</Label>
    <Input id={id('title')} bind:value={form.title} maxlength={300} />
  </div>
  {#if def.date_label !== null}
    <div class="flex flex-col gap-1">
      <Label for={id('date')}>{def.date_label}</Label>
      <Input id={id('date')} type="date" bind:value={form.occurred_on} />
    </div>
  {/if}
  {#each def.fields as f (f.key)}
    {#if f.type === 'long_text'}
      <div class="flex flex-col gap-1 sm:col-span-2">
        <Label for={id(f.key)}>{f.label}</Label>
        <textarea
          id={id(f.key)}
          class={box}
          rows={3}
          maxlength={10000}
          bind:value={form.values[f.key]}></textarea>
      </div>
    {:else if f.type === 'choice' || f.type === 'person' || f.type === 'course'}
      {@const options =
        f.type === 'choice'
          ? f.choices.map((c) => ({ value: c.value, label: c.label }))
          : (f.type === 'person' ? people : courses).map((p) => ({ value: p.id, label: p.name }))}
      <div class="flex flex-col gap-1">
        <Label for={id(f.key)}>{f.label}</Label>
        <select id={id(f.key)} class={['h-9', box]} bind:value={form.values[f.key]}>
          <option value="">선택 안 함</option>
          {#each options as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
        </select>
      </div>
    {:else if f.type === 'people'}
      <fieldset class="flex flex-col gap-1 rounded-md border p-2 sm:col-span-2">
        <legend class="px-1 text-sm font-medium">{f.label}</legend>
        <div class="flex flex-wrap gap-3">
          {#each people as p (p.id)}
            <div class="flex items-center gap-1">
              <Checkbox
                id={id(`${f.key}-${p.id}`)}
                checked={(form.people[f.key] ?? []).includes(p.id)}
                onCheckedChange={(on: boolean) => {
                  togglePerson(f.key, p.id, on);
                }}
              />
              <Label for={id(`${f.key}-${p.id}`)}>{p.name}</Label>
            </div>
          {/each}
        </div>
      </fieldset>
    {:else}
      <div class="flex flex-col gap-1">
        <Label for={id(f.key)}>{f.label}{f.type === 'tags' ? '(쉼표로 여럿)' : ''}</Label>
        <Input
          id={id(f.key)}
          type={f.type === 'date'
            ? 'date'
            : f.type === 'number'
              ? 'number'
              : f.type === 'url'
                ? 'url'
                : 'text'}
          min={f.type === 'number' ? 0 : undefined}
          step={f.type === 'number' ? 'any' : undefined}
          maxlength={f.type === 'url' ? 2000 : 300}
          bind:value={form.values[f.key]}
        />
      </div>
    {/if}
  {/each}
  <div class="flex flex-col gap-1 sm:col-span-2">
    <Label for={id('body')}>메모</Label>
    <textarea id={id('body')} class={box} rows={3} maxlength={20000} bind:value={form.body}
    ></textarea>
  </div>
  {#if def.file_roles.length > 0}
    <fieldset class="flex flex-col gap-2 rounded-md border p-2 sm:col-span-2">
      <legend class="px-1 text-sm font-medium">연결한 파일(이 항목의 첨부, 버전 고정)</legend>
      {#if files.length === 0}
        <p class="text-sm text-muted-foreground">
          이 항목에 첨부한 파일이 없습니다. 아래 "첨부 파일"에서 먼저 올리세요.
        </p>
      {/if}
      {#each form.files as link, index (index)}
        <div class="flex flex-wrap items-end gap-2">
          <div class="flex flex-col gap-1">
            <Label for={id(`file-${String(index)}`)}>파일</Label>
            <select
              id={id(`file-${String(index)}`)}
              class={['h-9', box]}
              bind:value={link.file_id}
              onchange={() => {
                link.version = '';
              }}
            >
              <option value="">선택</option>
              {#each files as file (file.id)}<option value={file.id}>{file.name}</option>{/each}
            </select>
          </div>
          <div class="flex flex-col gap-1">
            <Label for={id(`version-${String(index)}`)}>버전</Label>
            <select
              id={id(`version-${String(index)}`)}
              class={['h-9', box]}
              bind:value={link.version}
            >
              <option value="">지금 버전</option>
              {#each versionsOf(link.file_id) as v (v)}<option value={String(v)}>v{v}</option
                >{/each}
            </select>
          </div>
          <div class="flex flex-col gap-1">
            <Label for={id(`role-${String(index)}`)}>역할</Label>
            <select id={id(`role-${String(index)}`)} class={['h-9', box]} bind:value={link.role}>
              {#each def.file_roles as r (r.value)}<option value={r.value}>{r.label}</option>{/each}
            </select>
          </div>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            onclick={() => {
              form.files = form.files.filter((_, i) => i !== index);
            }}>빼기</Button
          >
        </div>
      {/each}
      {#if files.length > 0}
        <div>
          <Button
            type="button"
            size="sm"
            variant="outline"
            onclick={() => {
              form.files = [
                ...form.files,
                { file_id: '', version: '', role: def.file_roles[0]?.value ?? 'other' },
              ];
            }}>파일 연결</Button
          >
        </div>
      {/if}
    </fieldset>
  {/if}
  <div class="flex gap-2 sm:col-span-2">
    <Button type="submit">{submitLabel}</Button>
    <Button type="button" variant="ghost" onclick={oncancel}>취소</Button>
  </div>
</form>
