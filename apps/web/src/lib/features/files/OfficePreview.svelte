<script lang="ts">
  // Office 문서 미리보기(D-05, D-71). 내려받을 수 있는 사람에게만 보인다(바이트를 브라우저로 받아 그리므로).
  import { fileContentUrl } from '$lib/api/client';
  import { Button } from '$lib/ui/button';

  import type { OfficeKind, SheetView } from './office';
  import { docxSrcdoc, MAX_SHEET_ROWS, OFFICE_PREVIEW_MAX_BYTES, xlsxSheets } from './office';

  interface Props {
    fileId: string;
    name: string;
    size: number;
    kind: OfficeKind;
    onclose: () => void;
  }

  const { fileId, name, size, kind, onclose }: Props = $props();

  let srcdoc = $state<string | undefined>(undefined);
  let sheets = $state<SheetView[] | undefined>(undefined);
  let active = $state(0);
  let error = $state<string | undefined>(undefined);
  // 늦게 끝난 이전 요청이 결과를 덮지 않게 요청마다 번호를 붙인다.
  let seq = 0;

  $effect(() => {
    const id = fileId;
    const mine = ++seq;
    srcdoc = undefined;
    sheets = undefined;
    error = undefined;
    if (size > OFFICE_PREVIEW_MAX_BYTES) {
      error = '파일이 커서 미리볼 수 없습니다. 내려받아 여세요.';
      return;
    }
    void (async () => {
      try {
        const response = await fetch(fileContentUrl(id), { credentials: 'same-origin' });
        if (!response.ok) throw new Error(String(response.status));
        const bytes = await response.arrayBuffer();
        if (kind === 'docx') {
          const html = await docxSrcdoc(bytes);
          if (mine === seq) srcdoc = html;
        } else {
          const result = await xlsxSheets(bytes);
          if (mine === seq) sheets = result;
        }
      } catch {
        if (mine === seq) error = '미리보기를 만들지 못했습니다. 내려받아 여세요.';
      }
    })();
    return () => {
      seq += 1;
    };
  });
</script>

<section class="flex flex-col gap-2 rounded-md border p-2" aria-label={`${name} 미리보기`}>
  <div class="flex items-center gap-2">
    <h3 class="min-w-0 flex-1 truncate text-sm font-semibold">{name}</h3>
    <Button size="sm" variant="ghost" onclick={onclose}>닫기</Button>
  </div>
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive">{error}</p>
  {:else if srcdoc !== undefined}
    <!-- 문서 내용은 스크립트를 돌릴 수 없는 iframe 안에서만 보인다. -->
    <iframe
      title={`${name} 미리보기`}
      sandbox=""
      {srcdoc}
      class="h-[70vh] w-full rounded border bg-muted"
    ></iframe>
  {:else if sheets !== undefined}
    {#if sheets.length > 1}
      <div class="flex flex-wrap gap-1" role="tablist" aria-label="시트">
        {#each sheets as sheet, i (sheet.name)}
          <Button
            size="sm"
            variant={i === active ? 'secondary' : 'ghost'}
            role="tab"
            aria-selected={i === active}
            onclick={() => {
              active = i;
            }}>{sheet.name}</Button
          >
        {/each}
      </div>
    {/if}
    {@const sheet = sheets[active]}
    {#if sheet !== undefined}
      <div class="max-h-[70vh] overflow-auto rounded border" role="tabpanel">
        <table class="text-sm">
          <tbody>
            {#each sheet.rows as row, r (r)}
              <tr>
                {#each row as cell, c (c)}
                  <td class="border px-2 py-1 whitespace-nowrap">{cell ?? ''}</td>
                {/each}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
      {#if sheet.rows.length >= MAX_SHEET_ROWS}
        <p class="text-xs text-muted-foreground">
          앞 {MAX_SHEET_ROWS}행만 보입니다. 전체는 내려받아 여세요.
        </p>
      {/if}
      <p class="text-xs text-muted-foreground">
        셀 값만 보입니다(서식·그림·차트·병합은 보이지 않음).
      </p>
    {/if}
  {:else}
    <p aria-live="polite" class="text-sm">미리보기를 만드는 중…</p>
  {/if}
</section>
