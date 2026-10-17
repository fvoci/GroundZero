<script lang="ts">
  // Office 문서 미리보기(D-05, D-71). 내려받을 수 있는 사람에게만 보인다(바이트를 브라우저로 받아 그리므로).
  import { fileContentUrl } from '$lib/api/client';
  import { Button } from '$lib/ui/button';

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
  // 늦게 끝난 이전 요청이 결과를 덮지 않게 요청마다 번호를 붙인다.
  let seq = 0;

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
      else if (data.type === 'failed') error = '미리보기를 만들지 못했습니다. 내려받아 여세요.';
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
  {:else if pptxBytes !== undefined && pptxDoc !== undefined}
    <!-- 같은 출처가 아닌 격리 문서에서 그린다. 창 열기·이동·같은 출처 접근은 허용하지 않는다. -->
    <iframe
      bind:this={frame}
      title={`${name} 미리보기`}
      sandbox="allow-scripts"
      srcdoc={pptxDoc}
      class="h-[70vh] w-full rounded border bg-muted"
    ></iframe>
    <p aria-live="polite" class="text-xs text-muted-foreground">
      {pptxSlides === undefined ? '슬라이드를 그리는 중…' : `슬라이드 ${String(pptxSlides)}장`}
    </p>
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
