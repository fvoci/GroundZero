<script lang="ts">
  // 백링크로 들어온 원문 문서에서 대상을 가리키는 참조 자리로 간다(D-27). 편집기가 본문을 그린 뒤에 찾는다(실시간
  // 편집기는 늦게 그려지므로 바뀔 때마다 다시 찾는다). 여러 곳이면 몇 번째인지와 이전·다음, 정해진 시간 안에 못 찾으면
  // (그사이 참조가 지워졌거나 바뀜) 문서 처음을 그대로 보이며 알린다. 본문 선택·커서는 건드리지 않는다.
  import { untrack } from 'svelte';

  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  import type { ReferenceTarget } from './focus';
  import { referenceNodes } from './focus';

  interface Props {
    /** 본문 영역(`bind:this`라 그리기 전·바뀌는 사이에는 null일 수 있다). */
    root: HTMLElement | null | undefined;
    target: ReferenceTarget | undefined;
    /** 찾는 시간 한도(ms). */
    wait?: number;
  }

  const { root, target, wait = 3000 }: Props = $props();

  let found = $state<HTMLElement[]>([]);
  let index = $state(0);
  let phase = $state<'idle' | 'searching' | 'found' | 'missing'>('idle');
  let closed = $state(false);
  const MARK = ['ring-2', 'ring-primary', 'ring-offset-1'];

  function show(i: number): void {
    for (const el of found) el.classList.remove(...MARK);
    const el = found[i];
    if (el === undefined) return;
    index = i;
    el.classList.add(...MARK);
    el.scrollIntoView({ block: 'center' });
  }

  $effect(() => {
    const el = root;
    const t = target;
    closed = false;
    found = [];
    index = 0;
    if (el === undefined || el === null || t === undefined) {
      phase = 'idle';
      return;
    }
    phase = 'searching';
    let done = false;
    const look = (): void => {
      if (done) return;
      const list = referenceNodes(el, t);
      if (list.length === 0) return;
      done = true;
      observer.disconnect();
      clearTimeout(timer);
      found = list;
      phase = 'found';
      show(0);
    };
    const observer = new MutationObserver(look);
    observer.observe(el, { childList: true, subtree: true });
    const timer = setTimeout(() => {
      if (done) return;
      done = true;
      observer.disconnect();
      phase = 'missing';
    }, wait);
    untrack(look);
    return () => {
      done = true;
      observer.disconnect();
      clearTimeout(timer);
      for (const node of found) node.classList.remove(...MARK);
    };
  });
</script>

<!-- 알림 영역은 늘 두고(나중에 생긴 영역은 화면 낭독기가 놓칠 수 있다) 보일 때만 글을 넣는다. -->
<p role="status" class="sr-only" data-reference-focus-status>
  {#if !closed && phase === 'found'}
    {m.references_focus_found({ count: found.length, current: index + 1 })}
  {:else if !closed && phase === 'missing'}
    {m.references_focus_missing()}
  {/if}
</p>
{#if !closed && (phase === 'found' || phase === 'missing')}
  <div
    class="flex flex-wrap items-center gap-2 rounded-md border bg-muted/40 px-3 py-2 text-sm"
    data-reference-focus={phase}
  >
    <p class="flex-1" aria-hidden="true">
      {phase === 'found'
        ? m.references_focus_found({ count: found.length, current: index + 1 })
        : m.references_focus_missing()}
    </p>
    {#if phase === 'found' && found.length > 1}
      <Button
        size="sm"
        variant="outline"
        onclick={() => {
          show((index - 1 + found.length) % found.length);
        }}>{m.references_focus_previous()}</Button
      >
      <Button
        size="sm"
        variant="outline"
        onclick={() => {
          show((index + 1) % found.length);
        }}>{m.references_focus_next()}</Button
      >
    {/if}
    <Button
      size="sm"
      variant="ghost"
      onclick={() => {
        closed = true;
        for (const node of found) node.classList.remove(...MARK);
      }}>{m.references_focus_close()}</Button
    >
  </div>
{/if}
