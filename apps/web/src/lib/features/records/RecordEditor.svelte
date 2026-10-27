<script lang="ts">
  // 기록 입력 칸. 칸 정의(서버)대로 그린다. 파일 연결은 이 항목의 첨부와 그 버전을 고른다(D-35).
  import type { FileView, RecordKindView } from '$lib/api/client';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import { NativeSelect } from '$lib/ui/native-select';
  import { Textarea } from '$lib/ui/textarea';

  import { recordChoiceLabel, recordDateLabel, recordFieldLabel, recordRoleLabel } from './labels';
  import type { RecordForm } from './records';
  import { emptyForm } from './records';

  interface Props {
    def: RecordKindView;
    form?: RecordForm;
    people: { id: string; name: string }[];
    courses: { id: string; name: string }[];
    /** 이 강좌(항목)의 회차(D-123 질문·복습). 강의 항목이 아니면 빈 목록. */
    episodes?: { id: string; name: string }[];
    files: FileView[];
    submitLabel: string;
    onsubmit: () => void;
    oncancel: () => void;
    /** 저장하는 동안 잠근다(그사이 바뀐 입력은 성공한 저장을 버리게 한다). */
    disabled?: boolean;
  }

  let {
    def,
    form = $bindable(emptyForm()),
    people,
    courses,
    episodes = [],
    files,
    submitLabel,
    onsubmit,
    oncancel,
    disabled = false,
  }: Props = $props();

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
  class="grid gap-3 rounded-md border p-3 *:min-w-0 @xl:grid-cols-2"
  novalidate
  onsubmit={(e: SubmitEvent) => {
    e.preventDefault();
    onsubmit();
  }}
>
  <!-- 저장하는 동안 입력·단추를 잠근다. 격자 배치는 그대로 둔다(class="contents"). -->
  <fieldset class="contents *:min-w-0" {disabled}>
    <div class="flex flex-col gap-1 @xl:col-span-2">
      <Label for={id('title')}>{m.records_title()}</Label>
      <Input id={id('title')} bind:value={form.title} maxlength={300} />
    </div>
    {#if def.date_label !== null}
      <div class="flex flex-col gap-1">
        <Label for={id('date')}>{recordDateLabel(def)}</Label>
        <Input id={id('date')} type="date" bind:value={form.occurred_on} />
      </div>
    {/if}
    {#each def.fields as f (f.key)}
      {#if f.type === 'long_text'}
        <div class="flex flex-col gap-1 @xl:col-span-2">
          <Label for={id(f.key)}>{recordFieldLabel(def, f)}</Label>
          <Textarea
            id={id(f.key)}
            class="min-h-20"
            rows={3}
            maxlength={10000}
            bind:value={form.values[f.key]}
          />
        </div>
      {:else if f.type === 'choice' || f.type === 'person' || f.type === 'course' || f.type === 'episode'}
        {@const options =
          f.type === 'choice'
            ? f.choices.map((c) => ({ value: c.value, label: recordChoiceLabel(def, f, c) }))
            : (f.type === 'person' ? people : f.type === 'course' ? courses : episodes).map(
                (p) => ({
                  value: p.id,
                  label: p.name,
                }),
              )}
        <div class="flex flex-col gap-1">
          <Label for={id(f.key)}>{recordFieldLabel(def, f)}</Label>
          <NativeSelect id={id(f.key)} class="w-full" bind:value={form.values[f.key]}>
            <option value="">{m.records_not_selected()}</option>
            {#each options as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
          </NativeSelect>
        </div>
      {:else if f.type === 'people'}
        <fieldset class="flex flex-col gap-1 rounded-md border p-2 @xl:col-span-2">
          <legend class="px-1 text-sm font-medium">{recordFieldLabel(def, f)}</legend>
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
            <!-- 지금 멤버가 아닌 사람(나간 멤버)도 저장된 값이면 보여서 뺄 수 있게 한다(감사 W15). -->
            {#each (form.people[f.key] ?? []).filter((pid) => !people.some((p) => p.id === pid)) as pid (pid)}
              <div class="flex items-center gap-1">
                <Checkbox
                  id={id(`${f.key}-${pid}`)}
                  checked={true}
                  onCheckedChange={(on: boolean) => {
                    togglePerson(f.key, pid, on);
                  }}
                />
                <Label for={id(`${f.key}-${pid}`)}>{m.records_unknown_person()}</Label>
              </div>
            {/each}
          </div>
        </fieldset>
      {:else}
        <div class="flex flex-col gap-1">
          <Label for={id(f.key)}
            >{f.type === 'tags'
              ? m.records_tags_label({ label: recordFieldLabel(def, f) })
              : recordFieldLabel(def, f)}</Label
          >
          {#if f.type === 'number'}
            <!-- text preserves incomplete numeric input (for example "1e") across draft restoration. -->
            <Input
              id={id(f.key)}
              type="text"
              inputmode="decimal"
              value={form.values[f.key] ?? ''}
              aria-invalid={form.unreadable[f.key] === true ? true : undefined}
              oninput={(e: Event) => {
                const target = e.currentTarget;
                if (!(target instanceof HTMLInputElement)) return;
                form.values[f.key] = target.value;
                const value = target.value.trim();
                form.unreadable[f.key] =
                  value !== '' && (!Number.isFinite(Number(value)) || Number(value) < 0);
              }}
            />
          {:else}
            <Input
              id={id(f.key)}
              type={f.type === 'date' ? 'date' : f.type === 'url' ? 'url' : 'text'}
              maxlength={f.type === 'url' ? 2000 : 300}
              bind:value={form.values[f.key]}
            />
          {/if}
        </div>
      {/if}
    {/each}
    <div class="flex flex-col gap-1 @xl:col-span-2">
      <Label for={id('body')}>{m.records_body()}</Label>
      <Textarea
        id={id('body')}
        class="min-h-20"
        rows={3}
        maxlength={20000}
        bind:value={form.body}
      />
    </div>
    {#if def.file_roles.length > 0}
      <fieldset class="flex flex-col gap-2 rounded-md border p-2 @xl:col-span-2">
        <legend class="px-1 text-sm font-medium">{m.records_files_legend()}</legend>
        {#if files.length === 0}
          <p class="text-sm text-muted-foreground">
            {m.records_no_attachments()}
          </p>
        {/if}
        {#each form.files as link, index (index)}
          <div class="flex flex-wrap items-end gap-2">
            <div class="flex flex-col gap-1">
              <Label for={id(`file-${String(index)}`)}>{m.records_file()}</Label>
              <NativeSelect
                id={id(`file-${String(index)}`)}
                class="w-full"
                bind:value={link.file_id}
                onchange={() => {
                  link.version = '';
                }}
              >
                <option value="">{m.records_select()}</option>
                {#each files as file (file.id)}<option value={file.id}>{file.name}</option>{/each}
              </NativeSelect>
            </div>
            <div class="flex flex-col gap-1">
              <Label for={id(`version-${String(index)}`)}>{m.records_version()}</Label>
              <NativeSelect
                id={id(`version-${String(index)}`)}
                class="w-full"
                bind:value={link.version}
              >
                <option value="">{m.records_current_version()}</option>
                {#each versionsOf(link.file_id) as v (v)}<option value={String(v)}>v{v}</option
                  >{/each}
              </NativeSelect>
            </div>
            <div class="flex flex-col gap-1">
              <Label for={id(`role-${String(index)}`)}>{m.records_role()}</Label>
              <NativeSelect id={id(`role-${String(index)}`)} class="w-full" bind:value={link.role}>
                {#each def.file_roles as r (r.value)}<option value={r.value}
                    >{recordRoleLabel(def, r)}</option
                  >{/each}
              </NativeSelect>
            </div>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              onclick={() => {
                form.files = form.files.filter((_, i) => i !== index);
              }}>{m.records_unlink()}</Button
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
              }}>{m.records_link_file()}</Button
            >
          </div>
        {/if}
      </fieldset>
    {/if}
    <div class="flex gap-2 @xl:col-span-2">
      <Button type="submit">{submitLabel}</Button>
      <Button type="button" variant="ghost" onclick={oncancel}>{m.common_cancel()}</Button>
    </div>
  </fieldset>
</form>
