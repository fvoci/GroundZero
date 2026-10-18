<script lang="ts">
  // 블록 왼쪽의 + 단추와 손잡이. 마우스를 올린 블록, 아니면 커서가 있는 블록(키보드·터치)을 따른다.
  // +: 아래에 빈 블록을 넣고 / 메뉴를 연다. 손잡이: 끌어서 옮기기(ProseMirror 기본 끌어 놓기)와 메뉴
  // (위·아래로 옮기기, 복제, 지우기, 유형 바꾸기). 모든 조작은 block-ops의 한 트랜잭션이다.
  // 위치는 늘 "블록 시작"(블록 노드 바로 앞)으로 들고, 문서가 바뀌면 트랜잭션의 위치 대응으로 옮긴다.
  import type { Editor } from '@tiptap/core';
  import GripVertical from '@lucide/svelte/icons/grip-vertical';
  import Plus from '@lucide/svelte/icons/plus';
  import type { Transaction } from '@tiptap/pm/state';

  import { m } from '$lib/paraglide/messages.js';
  import * as DropdownMenu from '$lib/ui/dropdown-menu';

  import type { BlockRange } from './block-ops';
  import {
    blockAt,
    blockAtHit,
    blockStartingAt,
    focusBlock,
    runBlockOp,
    selectBlock,
  } from './block-ops';
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

  /** 마우스를 올린 블록의 시작. */
  let hoverFrom = $state<number | undefined>(undefined);
  /** 손잡이를 누른 순간부터 메뉴가 닫힐 때까지 고정한 블록의 시작(터치에서는 누르면 편집기 초점이 빠진다). */
  let pinnedFrom = $state<number | undefined>(undefined);
  let menuOpen = $state(false);
  let dragging: { slice: unknown; move: boolean } | undefined;

  const selectionBlockFrom = (): number | undefined =>
    blockAt(editor.state.doc, editor.state.selection.from)?.from;

  /** 지금 손잡이가 붙을 블록. */
  const target = $derived.by((): { block: BlockRange; top: number } | undefined => {
    if (version < 0 || !editor.isEditable) return undefined;
    // 초점은 Tiptap이 초점·흐림 때도 트랜잭션을 내므로(version) 그때마다 편집기에 직접 묻는다.
    const focused = editor.view.hasFocus();
    const from =
      menuOpen || pinnedFrom !== undefined
        ? (pinnedFrom ?? selectionBlockFrom())
        : (hoverFrom ?? (focused ? selectionBlockFrom() : undefined));
    if (from === undefined) return undefined;
    const block = blockStartingAt(editor.state.doc, from);
    if (block === undefined) return undefined;
    const dom = editor.view.nodeDOM(block.from);
    if (!(dom instanceof HTMLElement)) return undefined;
    const top = dom.getBoundingClientRect().top - container.getBoundingClientRect().top;
    return { block, top };
  });

  const index = $derived(target?.block.index ?? 0);
  const last = $derived(target === undefined ? 0 : target.block.parent.childCount - 1);
  // 유형 바꾸기는 글을 담는 블록(문단·제목·코드)과 목록 항목에만. 구분선·이미지·표·수식에는 없다.
  const canTurnInto = $derived(
    target !== undefined &&
      (target.block.node.isTextblock ||
        target.block.node.type.name === 'listItem' ||
        target.block.node.type.name === 'taskItem' ||
        target.block.node.type.name === 'blockquote'),
  );

  $effect(() => {
    const view = editor.view;
    const onMove = (event: MouseEvent): void => {
      const rect = view.dom.getBoundingClientRect();
      if (event.clientY < rect.top || event.clientY > rect.bottom) {
        hoverFrom = undefined;
        return;
      }
      // 손잡이 쪽(왼쪽 여백)에 있어도 그 줄의 블록을 찾도록 글자 영역 안쪽 x로 묻는다.
      const hit = view.posAtCoords({ left: rect.left + 16, top: event.clientY });
      hoverFrom = hit === null ? undefined : blockAtHit(view.state.doc, hit)?.from;
    };
    const onLeave = (): void => {
      hoverFrom = undefined;
    };
    // 다른 사람의 편집 등으로 문서가 바뀌면 고정한 블록 위치를 옮긴다. 그 블록이 지워졌으면 메뉴를 닫는다.
    const onTransaction = ({ transaction }: { transaction: Transaction }): void => {
      if (!transaction.docChanged) return;
      const pinned = pinnedFrom;
      queueMicrotask(() => {
        hoverFrom = undefined;
        if (pinned === undefined || pinnedFrom !== pinned) return;
        const mapped = transaction.mapping.mapResult(pinned, 1);
        if (mapped.deleted || blockStartingAt(editor.state.doc, mapped.pos) === undefined) {
          menuOpen = false;
          pinnedFrom = undefined;
        } else {
          pinnedFrom = mapped.pos;
        }
      });
    };
    container.addEventListener('mousemove', onMove);
    container.addEventListener('mouseleave', onLeave);
    editor.on('transaction', onTransaction);
    return () => {
      container.removeEventListener('mousemove', onMove);
      container.removeEventListener('mouseleave', onLeave);
      editor.off('transaction', onTransaction);
    };
  });

  // Ctrl/⌘+/ : 커서가 있는 블록의 메뉴를 연다.
  $effect(() => {
    if (menuRequest === undefined) return;
    pinnedFrom = blockAt(editor.state.doc, menuRequest)?.from;
    menuOpen = pinnedFrom !== undefined;
    onmenuhandled();
  });

  /** 누르기가 메뉴를 열지 않고 끝나면(오른쪽 단추, 터치가 스크롤로 바뀜 등) 고정을 푼다. */
  function releaseIfIdle(): void {
    setTimeout(() => {
      if (!menuOpen && dragging === undefined) pinnedFrom = undefined;
    }, 0);
  }

  function act(op: 'up' | 'down' | 'duplicate' | 'delete' | 'below'): void {
    if (target !== undefined) runBlockOp(editor, op, target.block);
  }

  /** + : 아래에 빈 블록을 넣고 / 를 써서 블록 메뉴를 연다. */
  function addBelow(block: BlockRange): void {
    if (runBlockOp(editor, 'below', block)) editor.chain().focus().insertContent('/').run();
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
    const handle = { slice, move: true };
    dragging = handle;
    view.dragging = handle;
  }

  /** 끌기가 편집기 밖에서 끝나거나 취소되면(Esc) ProseMirror가 지우지 못한 끌기 상태를 지운다. */
  function onDragEnd(): void {
    const view = editor.view;
    if (dragging !== undefined && view.dragging === dragging) view.dragging = null;
    dragging = undefined;
    if (!menuOpen) pinnedFrom = undefined;
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
        addBelow(current.block);
      }}
    >
      <Plus class="size-4" aria-hidden="true" />
    </button>
    <DropdownMenu.Root
      bind:open={menuOpen}
      onOpenChange={(open: boolean) => {
        if (open) pinnedFrom = current.block.from;
        else pinnedFrom = undefined;
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
            aria-keyshortcuts="Control+/ Meta+/"
            title={`${m.documents_block_menu()} — ${m.documents_block_keys()}`}
            onpointerdowncapture={() => {
              pinnedFrom = current.block.from;
            }}
            onpointerupcapture={releaseIfIdle}
            onpointercancel={releaseIfIdle}
            oncontextmenu={releaseIfIdle}
            ondragstart={(e: DragEvent) => {
              onDragStart(e, current.block);
            }}
            ondragend={onDragEnd}
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
          <DropdownMenu.SubTrigger disabled={!canTurnInto}
            >{m.documents_block_turn_into()}</DropdownMenu.SubTrigger
          >
          <DropdownMenu.SubContent class="min-w-44">
            {#each BLOCK_KINDS as kind (kind)}
              <DropdownMenu.Item
                onSelect={() => {
                  focusBlock(editor, current.block.from + 1);
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
