<script lang="ts">
  // 새 문서를 템플릿에서 시작하기(D-27). 고를 템플릿이 없으면 아무것도 보이지 않는다.
  import { api } from '$lib/api/client';

  interface Props {
    /** 이 워크스페이스의 템플릿과 내 개인 노트 템플릿. null이면 개인 노트 템플릿만. */
    workspaceId: string | null;
    id: string;
    value?: string;
  }

  let { workspaceId, id, value = $bindable('') }: Props = $props();

  let templates = $state<
    { id: string; title: string; reference: string | null; personal: boolean }[]
  >([]);

  $effect(() => {
    const ws = workspaceId;
    let cancelled = false;
    api
      .documentTemplates(ws)
      .then((list) => {
        if (!cancelled) templates = list;
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  });
</script>

{#if templates.length > 0}
  <div class="flex flex-col gap-1 text-sm">
    <label for={id} class="font-medium">템플릿</label>
    <select
      {id}
      class="h-9 rounded-md border border-input bg-transparent px-3 text-base"
      bind:value
    >
      <option value="">빈 문서</option>
      {#each templates as t (t.id)}
        <option value={t.id}>{t.personal ? '내 노트 · ' : ''}{t.reference ?? ''} {t.title}</option>
      {/each}
    </select>
  </div>
{/if}
