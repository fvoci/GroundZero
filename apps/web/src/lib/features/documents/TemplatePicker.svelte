<script lang="ts">
  // 새 문서를 템플릿에서 시작하기(D-27). 고를 템플릿이 없으면 아무것도 보이지 않는다.
  import { api } from '$lib/api/client';
  import { m } from '$lib/paraglide/messages.js';

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
    // 워크스페이스가 바뀌면 앞 목록에서 고르지 못하게 비우고 새로 읽는다. 고른 값도 함께 비운다(새 목록을 읽는 동안이나
    // 읽지 못하면 선택 칸이 보이지 않는데도 앞 워크스페이스에서 고른 템플릿이 새 문서에 쓰였다).
    templates = [];
    value = '';
    api
      .documentTemplates(ws)
      .then((list) => {
        if (cancelled) return;
        templates = list;
        // 고른 값이 이 목록에 없으면(다른 워크스페이스의 템플릿 등) 보이지 않는 값으로 만들지 않게 비운다.
        if (value !== '' && !list.some((t) => t.id === value)) value = '';
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  });
</script>

{#if templates.length > 0}
  <div class="flex flex-col gap-1 text-sm">
    <label for={id} class="font-medium">{m.documents_template_label()}</label>
    <select
      {id}
      class="h-9 rounded-md border border-input bg-transparent px-3 text-base"
      bind:value
    >
      <option value="">{m.documents_template_blank()}</option>
      {#each templates as t (t.id)}
        <option value={t.id}
          >{t.personal ? `${m.documents_my_notes()} · ` : ''}{t.reference ?? ''} {t.title}</option
        >
      {/each}
    </select>
  </div>
{/if}
