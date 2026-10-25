<script lang="ts">
  // Office 문서 미리보기(D-05, D-71). 내려받을 수 있는 사람에게만 보인다(바이트를 브라우저로 받아 그리므로).
  import { untrack } from 'svelte';

  import { fileContentUrl } from '$lib/api/client';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  import type { HwpRun } from './hwp';
  import { HWP_PREVIEW_MAX_BYTES, renderHwp, rhwpBase } from './hwp';
  import type { OfficeKind, SheetView } from './office';
  import {
    docxSrcdoc,
    MAX_SHEET_ROWS,
    OFFICE_PREVIEW_MAX_BYTES,
    pptxSrcdoc,
    xlsxSheets,
  } from './office';

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
  // PPTX: 격리 프레임에 넘길 바이트와 프레임이 알려 준 상태.
  let pptxBytes = $state<ArrayBuffer | undefined>(undefined);
  let pptxDoc = $state<string | undefined>(undefined);
  let pptxSlides = $state<number | undefined>(undefined);
  let frame = $state<HTMLIFrameElement | undefined>(undefined);
  // HWP·HWPX(D-76): 작업자가 그린 쪽(SVG를 그림 주소로), 진행 상태, 취소.
  let hwpPages = $state<string[]>([]);
  let hwpStatus = $state<'loading' | 'done' | undefined>(undefined);
  let hwpNote = $state<string | undefined>(undefined);
  let hwpRun: HwpRun | undefined;
  // 늦게 끝난 이전 요청이 결과를 덮지 않게 요청마다 번호를 붙인다.
  let seq = 0;

  // 효과 안에서 불려도 쪽 목록을 의존으로 잡지 않는다(읽고 쓰는 반복을 막는다).
  function clearHwp(): void {
    untrack(() => {
      hwpRun?.cancel();
      hwpRun = undefined;
      for (const url of hwpPages) URL.revokeObjectURL(url);
      hwpPages = [];
      hwpStatus = undefined;
      hwpNote = undefined;
    });
  }

  async function previewHwp(bytes: ArrayBuffer, mine: number): Promise<void> {
    const base = await rhwpBase();
    if (mine !== seq) return;
    if (base === undefined) {
      error = m.files_hwp_missing();
      return;
    }
    hwpStatus = 'loading';
    const run = renderHwp(base, bytes, (index, svg) => {
      if (mine !== seq) return;
      const url = URL.createObjectURL(new Blob([svg], { type: 'image/svg+xml' }));
      const next = [...hwpPages];
      next[index] = url;
      hwpPages = next;
    });
    hwpRun = run;
    const outcome = await run.result;
    if (mine !== seq) return;
    hwpRun = undefined;
    if (outcome.kind === 'done') {
      hwpStatus = 'done';
      hwpNote =
        outcome.total > outcome.rendered
          ? m.files_hwp_partial({ rendered: outcome.rendered, total: outcome.total })
          : m.files_hwp_pages({ count: outcome.total });
    } else if (outcome.kind === 'timeout') {
      error = m.files_preview_timeout();
    } else if (outcome.kind === 'cancelled') {
      hwpStatus = 'done';
      hwpNote = m.files_preview_stopped();
    } else {
      error = m.files_preview_corrupt();
    }
  }

  // 바이트는 프레임마다 한 번만 보낸다(다시 알리는 'ready'에 또 보내지 않는다).
  let sentTo: Window | undefined;

  function sendPptx(): void {
    const target = frame?.contentWindow;
    if (!target || pptxBytes === undefined || sentTo === target) return;
    sentTo = target;
    // 격리 프레임은 출처가 없어(opaque) 대상 출처를 지정할 수 없다. 이 사람이 내려받을 수 있는 바이트만 보낸다.
    target.postMessage({ type: 'pptx', bytes: pptxBytes }, '*');
  }

  $effect(() => {
    const listener = (event: MessageEvent<unknown>): void => {
      if (event.source === null || event.source !== frame?.contentWindow) return;
      const data = event.data;
      if (typeof data !== 'object' || data === null || !('type' in data)) return;
      if (data.type === 'ready') sendPptx();
      else if (data.type === 'rendered' && 'slides' in data && typeof data.slides === 'number')
        pptxSlides = data.slides;
      else if (data.type === 'failed') error = m.files_preview_failed();
    };
    window.addEventListener('message', listener);
    return () => {
      window.removeEventListener('message', listener);
    };
  });

  $effect(() => {
    const id = fileId;
    const mine = ++seq;
    srcdoc = undefined;
    sheets = undefined;
    pptxBytes = undefined;
    pptxDoc = undefined;
    sentTo = undefined;
    pptxSlides = undefined;
    clearHwp();
    error = undefined;
    if (size > (kind === 'hwp' ? HWP_PREVIEW_MAX_BYTES : OFFICE_PREVIEW_MAX_BYTES)) {
      error = m.files_too_large();
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
        } else if (kind === 'hwp') {
          await previewHwp(bytes, mine);
        } else if (kind === 'pptx') {
          const doc = await pptxSrcdoc();
          if (mine === seq) {
            pptxBytes = bytes;
            pptxDoc = doc;
          }
        } else {
          const result = await xlsxSheets(bytes);
          if (mine === seq) sheets = result;
        }
      } catch {
        if (mine === seq) error = m.files_preview_failed();
      }
    })();
    return () => {
      seq += 1;
      clearHwp();
    };
  });
</script>

<section
  class="flex flex-col gap-2 rounded-md border p-2"
  aria-label={m.files_preview_aria({ name })}
>
  <div class="flex items-center gap-2">
    <h3 class="min-w-0 flex-1 truncate text-sm font-semibold">{name}</h3>
    <Button size="sm" variant="ghost" onclick={onclose}>{m.files_close()}</Button>
  </div>
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive">{error}</p>
  {:else if srcdoc !== undefined}
    <!-- 문서 내용은 스크립트를 돌릴 수 없는 iframe 안에서만 보인다. -->
    <iframe
      title={m.files_preview_aria({ name })}
      sandbox=""
      {srcdoc}
      class="h-[70vh] w-full rounded border bg-muted"
    ></iframe>
  {:else if pptxBytes !== undefined && pptxDoc !== undefined}
    <!-- 같은 출처가 아닌 격리 문서에서 그린다. 창 열기·이동·같은 출처 접근은 허용하지 않는다. -->
    <iframe
      bind:this={frame}
      title={m.files_preview_aria({ name })}
      sandbox="allow-scripts"
      srcdoc={pptxDoc}
      class="h-[70vh] w-full rounded border bg-muted"
    ></iframe>
    <p aria-live="polite" class="text-xs text-muted-foreground">
      {pptxSlides === undefined
        ? m.files_slides_rendering()
        : m.files_slides_count({ count: pptxSlides })}
    </p>
  {:else if hwpStatus !== undefined}
    <!-- 쪽마다 SVG를 그림으로만 보인다(SVG 안 스크립트·바깥 요청이 돌지 않음). -->
    <div
      class="flex max-h-[70vh] flex-col items-center gap-3 overflow-auto rounded border bg-muted p-3"
    >
      {#each hwpPages as url, i (url)}
        <img
          src={url}
          alt={m.files_page_alt({ page: i + 1 })}
          class="w-full max-w-3xl bg-white shadow"
        />
      {/each}
    </div>
    <div class="flex items-center gap-2 text-xs text-muted-foreground">
      <span aria-live="polite"
        >{hwpStatus === 'loading'
          ? m.files_hwp_rendering({ count: hwpPages.length })
          : (hwpNote ?? '')}</span
      >
      {#if hwpStatus === 'loading'}
        <Button size="sm" variant="ghost" onclick={() => hwpRun?.cancel()}>{m.files_stop()}</Button>
      {/if}
    </div>
  {:else if sheets !== undefined}
    {#if sheets.length > 1}
      <!-- 방향키 탭 규약을 따르지 않으므로 탭 역할 대신 눌림 상태가 있는 단추 묶음으로 둔다. -->
      <div class="flex flex-wrap gap-1" role="group" aria-label={m.files_sheets()}>
        {#each sheets as sheet, i (sheet.name)}
          <Button
            size="sm"
            variant={i === active ? 'secondary' : 'ghost'}
            aria-pressed={i === active}
            onclick={() => {
              active = i;
            }}>{sheet.name}</Button
          >
        {/each}
      </div>
    {/if}
    {@const sheet = sheets[active]}
    {#if sheet !== undefined}
      <div class="max-h-[70vh] overflow-auto rounded border" role="region" aria-label={sheet.name}>
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
          {m.files_sheet_rows_limit({ rows: MAX_SHEET_ROWS })}
        </p>
      {/if}
      <p class="text-xs text-muted-foreground">
        {m.files_sheet_values_only()}
      </p>
    {/if}
  {:else}
    <p aria-live="polite" class="text-sm">{m.files_preview_loading()}</p>
  {/if}
</section>
