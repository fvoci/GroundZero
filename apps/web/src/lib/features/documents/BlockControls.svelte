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
    handleOffset,
    mapBlockStartThrough,
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
  /** 지금 보이는 손잡이 줄. */
  let controls = $state<HTMLDivElement | undefined>(undefined);
  let dragging: { slice: unknown; move: boolean } | undefined;
  /** 손잡이를 마지막으로 누른 것이 마우스인지(마우스면 클릭으로 메뉴를 연다). */
  let pressedWithMouse = false;

  /** 메뉴 트리거(bits-ui)가 준 이벤트 처리기를 부른다. */
  function forward(props: unknown, name: 'onpointerdown' | 'onclick', event: Event): void {
    if (typeof props !== 'object' || props === null) return;
    const handler: unknown = Reflect.get(props, name);
    if (typeof handler === 'function') Reflect.apply(handler, undefined, [event]);
  }

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
    const box = dom.getBoundingClientRect();
    const top = box.top - container.getBoundingClientRect().top + handleOffset(box.height);
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
      // 보이는 손잡이 줄 높이 안에서는 그 블록을 놓지 않는다. 짧은 블록(구분선)에서 손잡이로 가는 길은 블록
      // 밖 여백을 지나므로, 다시 찾으면 다음 블록으로 바뀌어 손잡이가 달아난다(누를 때 고정하면 이미 늦다). 손잡이가
      // 마우스가 아니라 선택 블록으로 떠 있을 때(편집기 밖에서 처음 들어올 때)도 같다.
      if (controls !== undefined) {
        const band = controls.getBoundingClientRect();
        if (event.clientY >= band.top && event.clientY <= band.bottom) return;
      }
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
    // 실시간 편집은 바뀐 최상위 블록을 통째로 바꿔 넣으므로, 다른 사람이 같은 최상위 블록(같은 목록·표 등) 안을
    // 고치면 고정한 위치가 지워진 것으로 보고 메뉴를 닫는다(엉뚱한 블록을 조작하지 않는 쪽). 한 순간에 트랜잭션이
    // 여럿 오면(덧붙인 트랜잭션 포함) 위치 대응을 차례로 이어 붙이고 상태는 한 번만 쓴다. 그 사이에 고정한 블록이
    // 바뀌었으면 새 값에서 다시 시작한다.
    let pending: { base: number | undefined; pinned: number | undefined } | undefined;
    const onTransaction = ({
      transaction,
      appendedTransactions,
    }: {
      transaction: Transaction;
      appendedTransactions: Transaction[];
    }): void => {
      const steps = [transaction, ...appendedTransactions].filter((tr) => tr.docChanged);
      if (steps.length === 0) return;
      const start =
        pending !== undefined && pending.base === pinnedFrom ? pending.pinned : pinnedFrom;
      const pinned = start === undefined ? undefined : mapBlockStartThrough(steps, start);
      const scheduled = pending !== undefined;
      pending = { base: pinnedFrom, pinned };
      if (scheduled) return;
      queueMicrotask(() => {
        const result = pending;
        pending = undefined;
        hoverFrom = undefined;
        if (result?.base === undefined || pinnedFrom !== result.base) return;
        pinnedFrom = result.pinned;
        if (pinnedFrom === undefined) menuOpen = false;
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
    bind:this={controls}
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
            onpointerdown={(e: PointerEvent) => {
              // 마우스로 누르면 끌어 옮기기가 시작될 수 있어 메뉴는 누를 때가 아니라 클릭(놓을 때)에 연다.
              // 끌기로 끝나면 클릭이 오지 않아 메뉴가 열리지 않는다. 터치·펜은 메뉴 기본 동작을 따른다.
              pressedWithMouse = e.pointerType === 'mouse';
              if (!pressedWithMouse) forward(props, 'onpointerdown', e);
            }}
            onclick={(e: MouseEvent) => {
              if (e.detail > 0 && pressedWithMouse) {
                pinnedFrom = current.block.from;
                menuOpen = !menuOpen;
                if (!menuOpen) pinnedFrom = undefined;
              } else forward(props, 'onclick', e);
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
