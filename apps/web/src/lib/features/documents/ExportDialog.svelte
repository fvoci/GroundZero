<script lang="ts">
  // 문서 한 건 내보내기(D-75). 저장된 리비전을 골라 Markdown·ZIP(Markdown과 그림·첨부)·PDF(브라우저 인쇄)로 내보낸다.
  // 권한은 서버가 확인하고(내려받기와 같은 규칙) 화면은 `can_export`일 때만 보인다. 저장하지 않은 편집이 있으면
  // 먼저 저장하거나 저장된 리비전을 내보낼지 알린다. 변환 결과와 제한(옮기지 못한 블록, 넣지 못한 첨부)을 보여 준다.
  import { onDestroy, untrack } from 'svelte';

  import { formatInstant } from '$lib/display';
  import type { DocumentView, RevisionSummaryView } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import type { ReferenceContext } from '$lib/features/references/context';
  import { m } from '$lib/paraglide/messages.js';
  import {
    accountSafety,
    accountSwitching,
    notifyDone,
  } from '$lib/features/feedback/feedback.svelte';
  import { me as currentUser } from '$lib/me.svelte';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';
  import { Label } from '$lib/ui/label';
  import { NativeSelect } from '$lib/ui/native-select';

  import type { ExportFormat } from './export';
  import { exportFileName, prepareExport, saveBlob, toMarkdown, toZip } from './export';
  import { printDocumentHtml, printToPdf } from './export-print';
  import { standInLabel } from './revision-author';

  interface Props {
    doc: DocumentView;
    /** 편집기에 저장하지 않은 편집이 있는지. */
    unsaved: boolean;
    references: ReferenceContext;
    /** 여는 단추는 문서 화면의 "더 보기" 메뉴에 있다. */
    open?: boolean;
    /** 닫으면 초점을 돌려줄 곳("더 보기" 단추). 연 메뉴 항목은 닫히며 사라지므로 브라우저에 맡기지 않는다. */
    returnFocus?: HTMLElement | null;
  }

  let { doc, unsaved, references, open = $bindable(false), returnFocus = null }: Props = $props();

  let revisions = $state<RevisionSummaryView[]>([]);
  let selected = $state(0);
  let format = $state<ExportFormat>('markdown');
  let busy = $state(false);
  let error = $state<string | undefined>(undefined);
  let notes = $state<string[] | undefined>(undefined);
  let mounted = true;
  let operation = $state.raw<{
    generation: number;
    userId: string;
    controller: AbortController;
  }>();
  onDestroy(() => {
    mounted = false;
    operation?.controller.abort();
  });
  // SR-221-2: portal이 남아 있어도 안전 상태·닫기·사용자 변경에서 첨부와 인쇄 대기를 끊는다.
  $effect.pre(() => {
    const generation = accountSafety.generation;
    const userId = currentUser.userId;
    const hidden = accountSwitching();
    if (
      operation &&
      (!open || hidden || operation.generation !== generation || operation.userId !== userId)
    ) {
      operation.controller.abort();
    }
  });

  // 화면 문구만 번역한다. 내보낸 파일 내용(알림 주석, 자리표시 글자)은 export.ts가 언어와 상관없이 만든다.
  const FORMATS = $derived<{ value: ExportFormat; label: string; hint: string }[]>([
    { value: 'markdown', label: 'Markdown (.md)', hint: m.documents_export_markdown_hint() },
    {
      value: 'zip',
      label: m.documents_export_zip_label(),
      hint: m.documents_export_zip_hint(),
    },
    {
      value: 'pdf',
      label: m.documents_export_pdf_label(),
      hint: m.documents_export_pdf_hint(),
    },
  ]);

  const when = (iso: string): string => formatInstant(iso);
  /** 리비전을 남긴 사람. 대신 저장한 리비전은 실제 편집자와 함께 보인다(D-95). */
  function byline(rev: RevisionSummaryView): string {
    const label = standInLabel(rev) ?? rev.author_name;
    return label === null ? '' : ` · ${label}`;
  }
  const latest = $derived(selected === doc.revision);

  // 열 때마다 지난 결과·오류를 지우고 지금 리비전을 고른 뒤 리비전 목록을 다시 읽는다.
  $effect(() => {
    if (open) untrack(prepare);
  });

  function prepare(): void {
    error = undefined;
    notes = undefined;
    selected = doc.revision;
    void loadRevisions();
  }

  /** 모든 리비전(서버는 100개씩 나눠 준다). 오래된 리비전도 고를 수 있다. */
  async function loadRevisions(): Promise<void> {
    const all: RevisionSummaryView[] = [];
    try {
      let before: number | null = null;
      // 끝없이 돌지 않게 쪽 수와 번호 감소를 함께 본다.
      for (let i = 0; i < 200; i += 1) {
        const page = await api.revisions(doc.id, before);
        all.push(...page);
        const last = page.at(-1);
        if (last === undefined || last.revision <= 1) break;
        if (before !== null && last.revision >= before) break;
        before = last.revision;
      }
    } catch {
      // 목록을 못 받으면 받은 것까지(없으면 최신 리비전만) 고를 수 있다.
    }
    revisions = all;
  }

  async function run(): Promise<void> {
    if (busy || !mounted || !open || accountSwitching() || !doc.can_export) return;
    const started = {
      generation: accountSafety.generation,
      userId: currentUser.userId,
      controller: new AbortController(),
    };
    operation = started;
    const current = (): boolean =>
      mounted &&
      open &&
      !started.controller.signal.aborted &&
      !accountSwitching() &&
      started.generation === accountSafety.generation &&
      started.userId === currentUser.userId;
    busy = true;
    error = undefined;
    notes = undefined;
    try {
      const rev = await api.exportRevision(doc.id, selected, format);
      if (!current()) return;
      const prepared = await prepareExport(rev.title, rev.revision, rev.body, {
        format,
        hrefFor: references.hrefFor,
        origin: window.location.origin,
        signal: started.controller.signal,
      });
      if (!current()) return;
      if (format === 'pdf') {
        const html = await printDocumentHtml(prepared, when(rev.created_at));
        if (!current()) return;
        await printToPdf(html, started.controller.signal);
        if (!current()) return;
        notes = prepared.notes;
      } else {
        const { markdown, lossy } = toMarkdown(prepared);
        const blob =
          format === 'zip'
            ? await toZip(prepared, markdown)
            : new Blob([markdown], { type: 'text/markdown;charset=utf-8' });
        if (!current()) return;
        saveBlob(blob, exportFileName(rev.title, format));
        notes = [
          ...prepared.notes,
          ...(lossy.length > 0 ? [m.documents_export_lossy({ blocks: lossy.join(', ') })] : []),
        ];
      }
      notifyDone(format === 'pdf' ? m.documents_export_print_opened() : m.documents_exported());
    } catch (e: unknown) {
      if (!current()) return;
      error =
        e instanceof ApiError && e.status === 403
          ? m.documents_export_forbidden()
          : errorText(e, m.documents_export_failed());
    } finally {
      busy = false;
      operation = undefined;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content
    class="max-h-[calc(100dvh-2rem)] grid-cols-[minmax(0,1fr)] overflow-y-auto sm:max-w-lg"
    onCloseAutoFocus={(e: Event) => {
      if (returnFocus === null) return;
      e.preventDefault();
      returnFocus.focus();
    }}
  >
    <Dialog.Header>
      <Dialog.Title>{m.documents_export()}</Dialog.Title>
      <Dialog.Description>{m.documents_export_description()}</Dialog.Description>
    </Dialog.Header>
    <div class="flex flex-col gap-3 text-sm">
      <div class="flex flex-col gap-1">
        <Label for="export-revision">{m.documents_revision_label()}</Label>
        <NativeSelect id="export-revision" class="w-full" bind:value={selected}>
          {#if revisions.length === 0}
            <option value={doc.revision}
              >{m.documents_revision_latest({ revision: doc.revision })}</option
            >
          {/if}
          {#each revisions as rev (rev.revision)}
            <option value={rev.revision}>
              {rev.revision === doc.revision
                ? m.documents_revision_latest({ revision: rev.revision })
                : m.documents_revision_n({ revision: rev.revision })} · {when(
                rev.created_at,
              )}{byline(rev)}
            </option>
          {/each}
        </NativeSelect>
      </div>
      <!-- 안내는 늘 있는 상태 영역 안에서 바꾼다(리비전을 바꾸면 나타나고 사라진다). -->
      <div role="status" class="empty:hidden">
        {#if latest && unsaved}
          <p
            class="rounded-md border border-amber-300 bg-amber-50 p-2 text-amber-900 dark:border-amber-700 dark:bg-amber-950 dark:text-amber-200"
          >
            {m.documents_export_unsaved({ revision: doc.revision })}
          </p>
        {/if}
      </div>
      <div role="status" class="empty:hidden">
        {#if latest && doc.realtime}
          <p class="text-muted-foreground">
            {m.documents_export_live()}
          </p>
        {/if}
      </div>
      <fieldset class="flex flex-col gap-1">
        <legend class="mb-1 font-medium">{m.documents_export_format()}</legend>
        {#each FORMATS as f (f.value)}
          <label class="flex items-start gap-2">
            <input
              type="radio"
              name="export-format"
              value={f.value}
              bind:group={format}
              class="mt-1"
            />
            <span>
              {f.label}
              <span class="block text-xs text-muted-foreground">{f.hint}</span>
            </span>
          </label>
        {/each}
      </fieldset>
      {#if error !== undefined}
        <p role="alert" class="text-destructive">{error}</p>
      {/if}
      <div role="status" class="flex flex-col gap-1 empty:hidden">
        {#if notes !== undefined}
          {#if notes.length > 0}
            <ul class="list-disc pl-5 text-muted-foreground">
              {#each notes as note, i (i)}
                <li>{note}</li>
              {/each}
            </ul>
          {/if}
        {/if}
      </div>
    </div>
    <Dialog.Footer>
      <Button variant="ghost" onclick={() => (open = false)}>{m.documents_close()}</Button>
      <Button disabled={busy} onclick={() => void run()}>
        {busy
          ? m.documents_exporting()
          : latest && unsaved
            ? m.documents_export_saved_revision({ revision: selected })
            : m.documents_export()}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
