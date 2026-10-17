<script lang="ts">
  // 문서 한 건 내보내기(D-75). 저장된 리비전을 골라 Markdown·ZIP(Markdown과 그림·첨부)·PDF(브라우저 인쇄)로 내보낸다.
  // 권한은 서버가 확인하고(내려받기와 같은 규칙) 화면은 `can_export`일 때만 보인다. 저장하지 않은 편집이 있으면
  // 먼저 저장하거나 저장된 리비전을 내보낼지 알린다. 변환 결과와 제한(옮기지 못한 블록, 넣지 못한 첨부)을 보여 준다.
  import type { DocumentView, RevisionSummaryView } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
  import type { ReferenceContext } from '$lib/features/references/context';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';
  import { Label } from '$lib/ui/label';

  import type { ExportFormat } from './export';
  import { exportFileName, prepareExport, saveBlob, toMarkdown, toZip } from './export';
  import { printDocumentHtml, printToPdf } from './export-print';

  interface Props {
    doc: DocumentView;
    /** 편집기에 저장하지 않은 편집이 있는지. */
    unsaved: boolean;
    references: ReferenceContext;
  }

  const { doc, unsaved, references }: Props = $props();

  let open = $state(false);
  let revisions = $state<RevisionSummaryView[]>([]);
  let selected = $state(0);
  let format = $state<ExportFormat>('markdown');
  let busy = $state(false);
  let error = $state<string | undefined>(undefined);
  let notes = $state<string[] | undefined>(undefined);

  const FORMATS: { value: ExportFormat; label: string; hint: string }[] = [
    { value: 'markdown', label: 'Markdown (.md)', hint: '그림·첨부는 fvoci 주소로 연결됩니다.' },
    {
      value: 'zip',
      label: 'Markdown + 그림·첨부 (.zip)',
      hint: '내려받을 수 있는 그림·첨부를 함께 담고 상대 경로로 잇습니다.',
    },
    {
      value: 'pdf',
      label: 'PDF (인쇄)',
      hint: '인쇄 창에서 “PDF로 저장”을 고르세요. 이 기기의 글꼴로 만듭니다.',
    },
  ];

  const when = (iso: string): string => new Date(iso).toLocaleString();
  const latest = $derived(selected === doc.revision);

  function openDialog(): void {
    open = true;
    error = undefined;
    notes = undefined;
    selected = doc.revision;
    api
      .revisions(doc.id, null)
      .then((list) => {
        revisions = list;
      })
      .catch(() => {
        revisions = [];
      });
  }

  async function run(): Promise<void> {
    busy = true;
    error = undefined;
    notes = undefined;
    try {
      const rev = await api.exportRevision(doc.id, selected, format);
      const prepared = await prepareExport(rev.title, rev.revision, rev.body, {
        format,
        hrefFor: references.hrefFor,
        origin: window.location.origin,
      });
      if (format === 'pdf') {
        await printToPdf(await printDocumentHtml(prepared, when(rev.created_at)));
        notes = prepared.notes;
      } else {
        const { markdown, lossy } = toMarkdown(prepared);
        const blob =
          format === 'zip'
            ? await toZip(prepared, markdown)
            : new Blob([markdown], { type: 'text/markdown;charset=utf-8' });
        saveBlob(blob, exportFileName(rev.title, format));
        notes = [
          ...prepared.notes,
          ...(lossy.length > 0
            ? [
                `Markdown으로 그대로 옮기지 못한 블록: ${lossy.join(', ')} (파일 맨 위 주석에도 적었습니다)`,
              ]
            : []),
        ];
      }
    } catch (e: unknown) {
      error =
        e instanceof ApiError && e.status === 403
          ? '이 문서는 내보낼 수 없습니다(소유자가 복제·내려받기를 허용하지 않음).'
          : '내보내지 못했습니다.';
    } finally {
      busy = false;
    }
  }
</script>

<Button size="sm" variant="outline" onclick={openDialog}>내보내기</Button>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>내보내기</Dialog.Title>
      <Dialog.Description>저장된 리비전 하나를 파일로 내보냅니다.</Dialog.Description>
    </Dialog.Header>
    <div class="flex flex-col gap-3 text-sm">
      <div class="flex flex-col gap-1">
        <Label for="export-revision">리비전</Label>
        <select
          id="export-revision"
          class="h-9 rounded-md border bg-background px-2"
          bind:value={selected}
        >
          {#if revisions.length === 0}
            <option value={doc.revision}>리비전 {doc.revision} (최신)</option>
          {/if}
          {#each revisions as rev (rev.revision)}
            <option value={rev.revision}>
              리비전 {rev.revision}{rev.revision === doc.revision ? ' (최신)' : ''} · {when(
                rev.created_at,
              )}{rev.author_name === null ? '' : ` · ${rev.author_name}`}
            </option>
          {/each}
        </select>
      </div>
      {#if latest && unsaved}
        <p role="status" class="rounded-md border border-amber-300 bg-amber-50 p-2 text-amber-900">
          저장하지 않은 편집이 있습니다. 그 내용까지 내보내려면 먼저 저장하세요. 지금 내보내면
          저장된 리비전 {doc.revision}이 나갑니다.
        </p>
      {/if}
      {#if latest && doc.realtime}
        <p role="status" class="text-muted-foreground">
          실시간 편집 중인 문서는 리비전으로 남은 내용까지 내보냅니다(조용해지거나 모두 나가면
          리비전으로 남습니다).
        </p>
      {/if}
      <fieldset class="flex flex-col gap-1">
        <legend class="mb-1 font-medium">형식</legend>
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
      {#if notes !== undefined}
        <div role="status" class="flex flex-col gap-1">
          <p>{format === 'pdf' ? '인쇄 창을 열었습니다.' : '내보냈습니다.'}</p>
          {#if notes.length > 0}
            <ul class="list-disc pl-5 text-muted-foreground">
              {#each notes as note, i (i)}
                <li>{note}</li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}
    </div>
    <Dialog.Footer>
      <Button variant="ghost" onclick={() => (open = false)}>닫기</Button>
      <Button disabled={busy} onclick={() => void run()}>
        {busy
          ? '내보내는 중…'
          : latest && unsaved
            ? `저장된 리비전 ${String(selected)} 내보내기`
            : '내보내기'}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
