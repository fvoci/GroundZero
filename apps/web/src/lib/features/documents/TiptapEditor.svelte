<script lang="ts">
  // Tiptap 편집기 래퍼. 인스턴스는 마운트 때 한 번 만들고 해제 때 destroy한다.
  // 내용을 통째로 바꿀 때는 부모가 {#key}로 이 컴포넌트를 다시 만든다.
  import { Editor } from '@tiptap/core';
  import StarterKit from '@tiptap/starter-kit';
  import { onMount, untrack } from 'svelte';

  import type { DocNode } from '$lib/api/client';
  import { parseDocNode } from '$lib/api/client';
  import { Button } from '$lib/ui/button';

  interface Props {
    content: DocNode;
    editable: boolean;
    label: string;
    onchange?: (body: DocNode) => void;
  }

  const { content, editable, label, onchange }: Props = $props();

  interface Marks {
    bold: boolean;
    italic: boolean;
    heading: boolean;
    bulletList: boolean;
    codeBlock: boolean;
  }

  let element: HTMLDivElement | undefined = $state();
  let editor: Editor | undefined;
  let marks = $state<Marks>({
    bold: false,
    italic: false,
    heading: false,
    bulletList: false,
    codeBlock: false,
  });

  function readMarks(e: Editor): Marks {
    return {
      bold: e.isActive('bold'),
      italic: e.isActive('italic'),
      heading: e.isActive('heading', { level: 2 }),
      bulletList: e.isActive('bulletList'),
      codeBlock: e.isActive('codeBlock'),
    };
  }

  onMount(() => {
    if (element === undefined) return;
    const instance = new Editor({
      element,
      extensions: [StarterKit],
      content: untrack(() => ({ ...content })),
      editable: untrack(() => editable),
      editorProps: {
        attributes: {
          class: 'fvoci-prose',
          role: 'textbox',
          'aria-multiline': 'true',
          'aria-label': untrack(() => label),
        },
      },
      onUpdate: ({ editor: e }) => {
        onchange?.(parseDocNode(e.getJSON()));
      },
      onTransaction: ({ editor: e }) => {
        marks = readMarks(e);
      },
    });
    editor = instance;
    return () => {
      instance.destroy();
      editor = undefined;
    };
  });

  type Command = 'bold' | 'italic' | 'heading' | 'bulletList' | 'codeBlock';

  function run(command: Command): void {
    const chain = editor?.chain().focus();
    if (chain === undefined) return;
    switch (command) {
      case 'bold':
        chain.toggleBold().run();
        break;
      case 'italic':
        chain.toggleItalic().run();
        break;
      case 'heading':
        chain.toggleHeading({ level: 2 }).run();
        break;
      case 'bulletList':
        chain.toggleBulletList().run();
        break;
      case 'codeBlock':
        chain.toggleCodeBlock().run();
        break;
    }
  }

  const tools: { command: Command; label: string }[] = [
    { command: 'bold', label: '굵게' },
    { command: 'italic', label: '기울임' },
    { command: 'heading', label: '제목' },
    { command: 'bulletList', label: '목록' },
    { command: 'codeBlock', label: '코드' },
  ];
</script>

{#if editable}
  <div class="flex flex-wrap gap-1 border-b pb-2" role="toolbar" aria-label="서식">
    {#each tools as tool (tool.command)}
      <Button
        variant={marks[tool.command] ? 'secondary' : 'ghost'}
        size="sm"
        aria-pressed={marks[tool.command]}
        onclick={() => {
          run(tool.command);
        }}>{tool.label}</Button
      >
    {/each}
  </div>
{/if}
<div bind:this={element} class="min-h-[12rem] py-3" data-testid="editor"></div>
