<script lang="ts">
  // 블록 왼쪽의 + 단추와 손잡이. 마우스를 올린 블록, 아니면 커서가 있는 블록(키보드·터치)을 따른다.
  // +: 아래에 빈 블록을 넣고 / 메뉴를 연다. 손잡이: 끌어서 옮기기(ProseMirror 기본 끌어 놓기)와 메뉴
  // (위·아래로 옮기기, 복제, 지우기, 유형 바꾸기). 모든 조작은 block-ops의 한 트랜잭션이다.
  import type { Editor } from '@tiptap/core';
  import GripVertical from '@lucide/svelte/icons/grip-vertical';
  import Plus from '@lucide/svelte/icons/plus';

  import { m } from '$lib/paraglide/messages.js';
  import * as DropdownMenu from '$lib/ui/dropdown-menu';

  import type { BlockRange } from './block-ops';
  import { blockAt, focusBlock, runBlockOp, selectBlock } from './block-ops';
  import { BLOCK_KINDS, blockKindLabel, setBlockKind } from './block-types';

  interface Props {
    editor: Editor;
    /** 편집기와 손잡이를 함께 담는 상자(위치 기준). */
    container: HTMLElement;
    /** 편집기 상태가 바뀔 때마다 늘어난다(다시 계산). */
    version: number;
    /** Ctrl/⌘+/ 로 메뉴를 열 블록 위치. 열면 부모가 undefined로 되돌린다. */
    menuRequest: number | undefined;
    onmenuhandled: () => void;
  }

  const { editor, container, version, menuRequest, onmenuhandled }: Props = $props();

  // 끌어 옮기기는 마우스 같은 정밀한 포인터에서만(휴대 기기 브라우저는 HTML 끌어 놓기를 쓰지 않고, 길게 누르기와
  // 겹친다). 터치에서는 메뉴의 위·아래로 옮기기를 쓴다.
  const finePointer =
    typeof window.matchMedia === 'function' && window.matchMedia('(pointer: fine)').matches;

  let hoverPos = $state<number | undefined>(undefined);
  let menuOpen = $state(false);
  let menuPos = $state<number | undefined>(undefined);

  /** 지금 손잡이가 붙을 블록. 메뉴가 열려 있으면 연 블록에 고정한다. */
  const target = $derived.by((): { block: BlockRange; top: number } | undefined => {
    if (version < 0 || !editor.isEditable) return undefined;
    // 초점은 Tiptap이 초점·흐림 때도 트랜잭션을 내므로(version) 그때마다 편집기에 직접 묻는다.
    const focused = editor.view.hasFocus();
    // 손잡이를 누른 순간부터(menuPos) 메뉴가 닫힐 때까지 그 블록에 고정한다. 터치에서는 누르면 편집기 초점이 빠져
    // 손잡이가 사라지기 때문이다.
    const pos =
      menuOpen || menuPos !== undefined
        ? (menuPos ?? editor.state.selection.from)
        : (hoverPos ?? (focused ? editor.state.selection.from : undefined));
    if (pos === undefined) return undefined;
    const block = blockAt(editor.state.doc, pos);
    if (block === undefined) return undefined;
    const dom = editor.view.nodeDOM(block.from);
    if (!(dom instanceof HTMLElement)) return undefined;
    const top = dom.getBoundingClientRect().top - container.getBoundingClientRect().top;
    return { block, top };
  });

  const index = $derived(target?.block.index ?? 0);
  const last = $derived(target === undefined ? 0 : target.block.parent.childCount - 1);

  $effect(() => {
    const view = editor.view;
    const onMove = (event: MouseEvent): void => {
      const rect = view.dom.getBoundingClientRect();
      if (event.clientY < rect.top || event.clientY > rect.bottom) {
        hoverPos = undefined;
        return;
      }
      // 손잡이 쪽(왼쪽 여백)에 있어도 그 줄의 블록을 찾도록 글자 영역 안쪽 x로 묻는다.
      const hit = view.posAtCoords({ left: rect.left + 16, top: event.clientY });
      hoverPos = hit === null ? undefined : hit.inside >= 0 ? hit.inside + 1 : hit.pos;
    };
    const onLeave = (): void => {
      hoverPos = undefined;
    };
    container.addEventListener('mousemove', onMove);
    container.addEventListener('mouseleave', onLeave);
    return () => {
      container.removeEventListener('mousemove', onMove);
      container.removeEventListener('mouseleave', onLeave);
    };
  });

  // Ctrl/⌘+/ : 커서가 있는 블록의 메뉴를 연다.
  $effect(() => {
    if (menuRequest === undefined) return;
    // 메뉴 위치는 늘 "블록 안 첫 자리"(블록 시작 + 1)로 둔다. 커서가 줄 끝이어도 다음 블록으로 넘어가지 않게.
    const req = blockAt(editor.state.doc, menuRequest);
    menuPos = (req?.from ?? menuRequest) + 1;
    menuOpen = true;
    onmenuhandled();
  });

  function openMenuFor(blockFrom: number): void {
    menuPos = blockFrom + 1;
  }

  function act(op: 'up' | 'down' | 'duplicate' | 'delete' | 'below'): void {
    const pos = menuPos ?? (target === undefined ? undefined : target.block.from + 1);
    if (pos === undefined) return;
    runBlockOp(editor, op, pos);
  }

  /** + : 아래에 빈 블록을 넣고 / 를 써서 블록 메뉴를 연다. */
  function addBelow(pos: number): void {
    if (runBlockOp(editor, 'below', pos + 1)) editor.chain().focus().insertContent('/').run();
  }

  function onDragStart(event: DragEvent, block: BlockRange): void {
    if (event.dataTransfer === null) return;
    const view = editor.view;
    const tr = selectBlock(view.state, block);
    const slice = tr.selection.content();
    view.dispatch(tr);
    const { text } = view.serializeForClipboard(slice);
    event.dataTransfer.clearData();
    event.dataTransfer.setData('text/plain', text);
    event.dataTransfer.effectAllowed = 'copyMove';
    const dom = view.nodeDOM(block.from);
    if (dom instanceof HTMLElement) event.dataTransfer.setDragImage(dom, 0, 0);
    view.dragging = { slice, move: true };
  }
</script>

{#if target !== undefined}
  {@const current = target}
  <div
    class="absolute left-0 z-10 flex h-7 -translate-x-1 items-center gap-0.5 text-muted-foreground"
    style:top={`${String(current.top)}px`}
    data-testid="block-controls"
  >
    <button
      type="button"
      class="flex size-6 items-center justify-center rounded hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
      aria-label={m.documents_block_add()}
      title={m.documents_block_add()}
      onmousedown={(e: MouseEvent) => {
        e.preventDefault();
      }}
      onclick={() => {
        addBelow(current.block.from);
      }}
    >
      <Plus class="size-4" aria-hidden="true" />
    </button>
    <DropdownMenu.Root
      bind:open={menuOpen}
      onOpenChange={(open: boolean) => {
        if (open) openMenuFor(current.block.from);
        else menuPos = undefined;
      }}
    >
      <DropdownMenu.Trigger>
        {#snippet child({ props })}
          <button
            {...props}
            type="button"
            draggable={finePointer}
            class="flex size-6 cursor-grab items-center justify-center rounded hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            aria-label={m.documents_block_menu()}
            title={`${m.documents_block_menu()} — ${m.documents_block_keys()}`}
            onpointerdowncapture={() => {
              menuPos = current.block.from + 1;
            }}
            ondragstart={(e: DragEvent) => {
              onDragStart(e, current.block);
            }}
            ondragend={() => {
              if (!menuOpen) menuPos = undefined;
            }}
          >
            <GripVertical class="size-4" aria-hidden="true" />
          </button>
        {/snippet}
      </DropdownMenu.Trigger>
      <DropdownMenu.Content
        align="start"
        class="min-w-52"
        onCloseAutoFocus={(e: Event) => {
          // 메뉴를 닫으면 편집기로 돌아간다(손잡이에 초점이 남지 않게).
          e.preventDefault();
          editor.commands.focus();
        }}
      >
        <DropdownMenu.Item
          disabled={index === 0}
          onSelect={() => {
            act('up');
          }}
          >{m.documents_block_move_up()}<DropdownMenu.Shortcut>Alt⇧↑</DropdownMenu.Shortcut
          ></DropdownMenu.Item
        >
        <DropdownMenu.Item
          disabled={index >= last}
          onSelect={() => {
            act('down');
          }}
          >{m.documents_block_move_down()}<DropdownMenu.Shortcut>Alt⇧↓</DropdownMenu.Shortcut
          ></DropdownMenu.Item
        >
        <DropdownMenu.Item
          onSelect={() => {
            act('duplicate');
          }}>{m.documents_block_duplicate()}</DropdownMenu.Item
        >
        <DropdownMenu.Item
          onSelect={() => {
            act('below');
          }}>{m.documents_block_add()}</DropdownMenu.Item
        >
        <DropdownMenu.Sub>
          <DropdownMenu.SubTrigger>{m.documents_block_turn_into()}</DropdownMenu.SubTrigger>
          <DropdownMenu.SubContent class="min-w-44">
            {#each BLOCK_KINDS as kind (kind)}
              <DropdownMenu.Item
                onSelect={() => {
                  focusBlock(editor, menuPos ?? current.block.from + 1);
                  setBlockKind(editor, kind);
                }}>{blockKindLabel(kind)}</DropdownMenu.Item
              >
            {/each}
          </DropdownMenu.SubContent>
        </DropdownMenu.Sub>
        <DropdownMenu.Separator />
        <DropdownMenu.Item
          class="text-destructive"
          onSelect={() => {
            act('delete');
          }}>{m.documents_block_delete()}</DropdownMenu.Item
        >
      </DropdownMenu.Content>
    </DropdownMenu.Root>
  </div>
{/if}
