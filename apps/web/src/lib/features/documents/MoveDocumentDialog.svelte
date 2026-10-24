<script lang="ts">
  // 검색으로 상위 문서를 골라 옮기기 (D-27). 같은 트리(같은 자리)의 문서만 후보이고 자기와 하위 문서는 뺀다. 같은
  // 자리·자기 하위·깊이 한도는 서버가 다시 막으므로, 거절되면 그 이유를 이 창에 남기고 창을 닫지 않는다.
  // 키보드: 검색 입력에서 위·아래로 후보를 고르고 Enter로 옮긴다(combobox + listbox 패턴).
  import type { SummaryView } from '$lib/api/client';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';

  import { parentCandidates } from './tree';

  interface Props {
    docs: SummaryView[];
    /** 옮길 문서. 없으면 창을 닫는다. */
    docId: string | undefined;
    /** 고른 부모(null은 맨 위)로 옮긴다. 실패하면 알릴 글을, 성공하면 undefined를 돌려준다. */
    onconfirm: (parentId: string | null) => Promise<string | undefined>;
    onclose: (moved: boolean) => void;
  }

  const { docs, docId, onconfirm, onclose }: Props = $props();

  const doc = $derived(docs.find((d) => d.id === docId));
  let query = $state('');
  let active = $state(0);
  let busy = $state(false);
  let failure = $state<string | undefined>(undefined);
  let moved = false;

  // 맨 위로(상위 없음)는 지금 상위 문서가 있을 때만 후보다.
  const ROOT = '';
  const options = $derived.by(() => {
    if (doc === undefined) return [];
    const list = parentCandidates(docs, doc.id, query).map((c) => ({
      id: c.doc.id,
      title: c.doc.title,
      reference: c.doc.reference,
      path: c.path,
      current: c.doc.id === doc.parent_id,
    }));
    const root =
      doc.parent_id !== null && query.trim() === ''
        ? [
            {
              id: ROOT,
              title: m.documents_reparent_to_root(),
              reference: null,
              path: [],
              current: false,
            },
          ]
        : [];
    return [...root, ...list];
  });

  $effect(() => {
    // 창을 열 때마다 처음부터.
    if (docId === undefined) return;
    query = '';
    active = 0;
    failure = undefined;
    moved = false;
  });
  $effect(() => {
    if (active >= options.length) active = Math.max(0, options.length - 1);
  });

  const optionId = (index: number): string => `move-doc-option-${String(index)}`;

  async function choose(index: number): Promise<void> {
    const option = options[index];
    if (option === undefined || busy || option.current) return;
    busy = true;
    failure = undefined;
    try {
      const why = await onconfirm(option.id === ROOT ? null : option.id);
      if (why === undefined) {
        moved = true;
        onclose(true);
      } else {
        failure = why;
      }
    } finally {
      busy = false;
    }
  }

  function onkeydown(event: KeyboardEvent): void {
    if (event.isComposing) return;
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      active = Math.min(active + 1, options.length - 1);
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      active = Math.max(active - 1, 0);
    } else if (event.key === 'Enter') {
      event.preventDefault();
      void choose(active);
    }
  }
</script>

<Dialog.Root
  open={doc !== undefined}
  onOpenChange={(o: boolean) => {
    if (!o) onclose(moved);
  }}
>
  <Dialog.Content
    class="sm:max-w-lg"
    onCloseAutoFocus={(e: Event) => {
      // 닫히면 부르는 쪽(트리)이 옮긴 문서 줄로 초점을 돌린다.
      e.preventDefault();
    }}
  >
    <Dialog.Header>
      <Dialog.Title>{m.documents_reparent_title({ title: doc?.title ?? '' })}</Dialog.Title>
      <Dialog.Description>{m.documents_reparent_description()}</Dialog.Description>
    </Dialog.Header>
    <label class="flex flex-col gap-1 text-sm">
      {m.documents_reparent_search()}
      <input
        class="h-9 rounded-md border border-input bg-transparent px-2 text-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        role="combobox"
        aria-expanded="true"
        aria-controls="move-doc-options"
        aria-autocomplete="list"
        aria-activedescendant={options.length > 0 ? optionId(active) : undefined}
        bind:value={query}
        {onkeydown}
        readonly={busy}
        aria-busy={busy}
      />
    </label>
    {#if options.length === 0}
      <p class="text-sm text-muted-foreground" role="status">{m.documents_reparent_none()}</p>
    {/if}
    <ul
      id="move-doc-options"
      role="listbox"
      aria-label={m.documents_reparent_options()}
      class="flex max-h-72 flex-col gap-0.5 overflow-y-auto"
    >
      {#each options as option, index (option.id)}
        <li
          id={optionId(index)}
          role="option"
          aria-selected={index === active}
          aria-disabled={option.current}
          class={[
            'flex cursor-pointer flex-col rounded px-2 py-1 text-sm',
            index === active && 'bg-accent',
            option.current && 'cursor-default text-muted-foreground',
          ]}
          onmousedown={(e: MouseEvent) => {
            e.preventDefault();
          }}
          onclick={() => {
            active = index;
            void choose(index);
          }}
          onkeydown={(e: KeyboardEvent) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault();
              active = index;
              void choose(index);
            }
          }}
        >
          <span>
            {#if option.reference !== null}<span
                class="font-mono text-xs whitespace-nowrap text-muted-foreground"
                >{option.reference}</span
              >{/if}
            {option.title}
            {#if option.current}<span class="text-xs">({m.documents_reparent_current()})</span>{/if}
          </span>
          {#if option.path.length > 0}
            <span class="text-xs text-muted-foreground">{option.path.join(' › ')}</span>
          {/if}
        </li>
      {/each}
    </ul>
    {#if failure !== undefined}
      <p role="alert" class="text-sm text-destructive">{failure}</p>
    {/if}
    <!-- 고른 곳을 단추 옆에 글로 보인다(무엇을 누르면 어디로 가는지 확인하고 누르게). 후보가 없으면 위의 안내만. -->
    {@const target = options[active]}
    <p class="text-sm text-muted-foreground" aria-live="polite" data-testid="move-doc-target">
      {#if target === undefined}
        <!-- 비움 -->
      {:else if target.current}
        {m.documents_reparent_target_current()}
      {:else if target.id === ROOT}
        {m.documents_reparent_target_root()}
      {:else}
        {m.documents_reparent_target({ title: target.title })}
      {/if}
    </p>
    <Dialog.Footer>
      <Button
        variant="outline"
        onclick={() => {
          onclose(moved);
        }}>{m.documents_reparent_cancel()}</Button
      >
      <Button
        disabled={busy || options[active] === undefined || options[active]?.current === true}
        onclick={() => void choose(active)}>{m.documents_reparent_confirm()}</Button
      >
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
