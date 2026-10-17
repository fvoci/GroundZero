<script lang="ts">
  // 시간 기록의 연결 대상 고르기(업무·문서·항목). @ 링크와 같은 후보 검색을 쓴다(볼 수 있는 대상만 나온다).
  import type { ReferenceCandidateView, TimeTargetKind } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  interface Props {
    idPrefix: string;
    onpick: (target: { kind: TimeTargetKind; id: string; label: string }) => void;
  }

  const { idPrefix, onpick }: Props = $props();

  let query = $state('');
  let results = $state<ReferenceCandidateView[]>([]);
  let timer: ReturnType<typeof setTimeout> | undefined;
  // 늦게 온 이전 검색 결과가 새 결과를 덮지 않게 순번을 붙인다.
  let seq = 0;

  $effect(() => () => {
    clearTimeout(timer);
  });

  function search(q: string): void {
    clearTimeout(timer);
    if (q.trim() === '') {
      results = [];
      return;
    }
    const mine = ++seq;
    timer = setTimeout(() => {
      api
        .referenceCandidates(q.trim(), null)
        .then((found) => {
          if (mine === seq) results = found.filter((c) => c.kind !== 'user');
        })
        .catch(() => {
          results = [];
        });
    }, 200);
  }

  const labelOf = (c: ReferenceCandidateView): string =>
    `${c.reference ?? ''} ${c.title}`.trim() +
    (c.workspace_name === null ? '' : ` · ${c.workspace_name}`);
</script>

<div class="flex flex-col gap-1">
  <Label for={`${idPrefix}-target-search`}>연결할 업무·문서·항목 찾기</Label>
  <Input
    id={`${idPrefix}-target-search`}
    bind:value={query}
    autocomplete="off"
    placeholder="제목이나 번호(예: DEV-3)"
    oninput={() => {
      search(query);
    }}
  />
  {#if results.length > 0}
    <ul class="flex max-h-40 flex-col overflow-y-auto rounded-md border" aria-label="찾은 대상">
      {#each results as c (`${c.kind}:${c.id}`)}
        {#if c.kind !== 'user'}
          {@const kind = c.kind}
          <li>
            <button
              type="button"
              class="w-full px-2 py-1 text-left text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
              onclick={() => {
                onpick({ kind, id: c.id, label: labelOf(c) });
                query = '';
                results = [];
              }}>{labelOf(c)}</button
            >
          </li>
        {/if}
      {/each}
    </ul>
  {/if}
</div>
