<script lang="ts">
  // 편집기 대신: 열리면 받은 본문(맞춰진 서버 내용)을 넘기고, 누르면 고친 본문을 넘긴다(실시간 편집기 시험에서
  // Tiptap·커서 표시 없이 초안 남기기를 본다). 고칠 수 있는지와 파일을 넣을 수 있는지는 속성으로 드러낸다.
  import { onMount } from 'svelte';

  import type { DocNode } from '$lib/api/client';

  interface Props {
    content: DocNode;
    editable?: boolean;
    insertFile?: unknown;
    onchange?: (body: DocNode) => void;
  }

  const { content, editable = true, insertFile, onchange }: Props = $props();
  onMount(() => {
    onchange?.(content);
  });
</script>

<button
  type="button"
  data-editable={String(editable)}
  data-insert-file={String(insertFile !== undefined)}
  onclick={() => {
    onchange?.({
      type: 'doc',
      content: [{ type: 'paragraph', content: [{ type: 'text', text: '확인받지 못한 문장' }] }],
    });
  }}>stub-type</button
>
