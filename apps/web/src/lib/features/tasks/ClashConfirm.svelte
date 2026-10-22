<script lang="ts">
  // 충돌 화면의 "내 입력으로 다시 저장"에서 나와 다른 사람이 같은 칸을 서로 다르게 바꿨을 때의 확인(D-94). 칸마다
  // 서버의 지금 값과 내 값을 글로 보이고, 덮어쓸지 묻는다. 취소하면 아무것도 보내지 않는다(입력은 그대로).
  // 값은 줄이지 않고 줄바꿈·공백을 지켜 보인다(길면 칸 안에서 스크롤한다). 서로 다른 값이 같아 보이면 무엇을
  // 덮어쓰는지 알 수 없다.
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  import type { ClashRow } from './labels';

  interface Props {
    rows: ClashRow[];
    busy: boolean;
    onconfirm: () => void;
    oncancel: () => void;
  }

  const { rows, busy, onconfirm, oncancel }: Props = $props();

  // 뜨면 제목으로 초점을 옮겨 읽히게 한다(키보드로 바로 단추에 닿는다).
  let heading = $state<HTMLElement | undefined>(undefined);
  $effect(() => {
    heading?.focus();
  });

  // Esc는 취소와 같다(아무것도 보내지 않는다). 둘러싼 업무 대화상자는 문서에서 Esc를 받아 닫으므로 여기서 멈춰, 확인만
  // 닫히고 대화상자와 입력은 남게 한다. 처리기는 이 요소에 직접 달아, Svelte의 위임 처리기가 어느 뿌리(앱, 대화상자를
  // 그리는 포털)에 붙든 문서의 대화상자 처리기보다 먼저 받는다. 확인 상자는 tabindex="-1"이라 안내문·값 상자처럼
  // 단추가 아닌 곳을 눌러도 초점이 상자에 남는다(없으면 초점이 둘러싼 대화상자로 가서 Esc가 대화상자를 닫고 입력을
  // 버렸다, 리뷰).
  let box = $state<HTMLElement | undefined>(undefined);
  $effect(() => {
    const el = box;
    if (el === undefined) return;
    const onkeydown = (e: KeyboardEvent): void => {
      if (e.key !== 'Escape') return;
      e.preventDefault();
      e.stopPropagation();
      if (!busy) oncancel();
    };
    el.addEventListener('keydown', onkeydown);
    return () => {
      el.removeEventListener('keydown', onkeydown);
    };
  });

  // 넘쳐 스크롤되는 값 상자("칸:쪽"). 그 상자만 이름 있는 영역으로 탭 순서에 넣는다: Safari는 Chrome·Firefox와 달리
  // 스크롤되는 영역에 키보드 초점을 주지 않아, 넘친 글을 키보드로 스크롤해 읽을 수 없었다(리뷰). 초점을 받은 영역은
  // 화살표·Page 키로 스크롤된다. 짧은 상자에는 역할·탭 정지를 더하지 않는다. 값(`rows`)이 바뀌거나 크기가 바뀌면 다시 잰다.
  const SIDES = ['server', 'mine'] as const;
  let scrolling = $state<ReadonlySet<string>>(new Set());
  $effect(() => {
    const el = box;
    const keys = rows.map((row) => row.key);
    if (el === undefined) return;
    const boxes = keys.flatMap((key) =>
      SIDES.flatMap((side) => {
        const value = el.querySelector<HTMLElement>(
          `[data-clash-field="${key}"] [data-clash-side="${side}"] > [data-clash-value]`,
        );
        return value === null ? [] : [{ id: `${key}:${side}`, value }];
      }),
    );
    const measure = (): void => {
      scrolling = new Set(
        boxes
          .filter(
            ({ value }) =>
              value.scrollHeight > value.clientHeight || value.scrollWidth > value.clientWidth,
          )
          .map(({ id }) => id),
      );
    };
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(measure);
    for (const { value } of boxes) observer.observe(value);
    return () => {
      observer.disconnect();
    };
  });
</script>

<!--
  aria-modal은 두지 않는다: 이 확인은 충돌 알림 안에 놓인 인라인 확인이고, 그 밖의 충돌 알림(서버 내용 불러오기)과
  둘러싼 대화상자의 닫기는 그대로 쓸 수 있다. aria-modal="true"는 화면 읽기 프로그램에 그 바깥을 쓸 수 없다고 알려
  실제로 쓸 수 있는 단추를 숨긴다. 업무 상세의 나머지는 확인이 떠 있는 동안 inert로 실제로 막는다(TaskDetail).
-->
<div
  class="mt-3 flex flex-col gap-2 rounded-md border border-amber-500/50 p-3 outline-none"
  role="alertdialog"
  tabindex="-1"
  aria-labelledby="task-clash-heading"
  aria-describedby="task-clash-hint"
  bind:this={box}
>
  <h3 id="task-clash-heading" class="font-semibold" tabindex="-1" bind:this={heading}>
    {m.tasks_clash_heading()}
  </h3>
  <p id="task-clash-hint" class="text-muted-foreground">{m.tasks_clash_hint()}</p>
  <ul class="flex flex-col gap-3">
    {#each rows as row (row.key)}
      <li data-clash-field={row.key} class="flex flex-col gap-1">
        <span class="font-medium" id={`task-clash-${row.key}-label`}>{row.label}</span>
        <dl class="grid gap-2 sm:grid-cols-2">
          {#each SIDES as side (side)}
            {@const scrolls = scrolling.has(`${row.key}:${side}`)}
            <div class="flex min-w-0 flex-col gap-0.5">
              <dt class="text-muted-foreground" id={`task-clash-${row.key}-${side}`}>
                {side === 'server' ? m.tasks_clash_server() : m.tasks_clash_mine()}
              </dt>
              <dd data-clash-side={side}>
                <!-- 넘칠 때만 탭 정지·영역 역할·이름(칸 이름과 어느 쪽 값)을 준다(위 설명). 규칙은 넘칠 때만 주는 이
                     tabindex도 막는다. 이름 있는 영역(role="region")에 초점을 주는 것은 스크롤 영역을 키보드로 읽게 하는
                     표준 처리다(WCAG 2.1.1, axe scrollable-region-focusable). 대안인 읽기 전용 textarea는 짧은 값에도 늘
                     탭 정지가 생기고 고칠 수 있는 칸처럼 읽혀 택하지 않았다. -->
                <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
                <div
                  data-clash-value
                  class="max-h-60 overflow-auto rounded bg-muted px-2 py-1 break-words whitespace-pre-wrap focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                  role={scrolls ? 'region' : undefined}
                  tabindex={scrolls ? 0 : undefined}
                  aria-labelledby={scrolls
                    ? `task-clash-${row.key}-label task-clash-${row.key}-${side}`
                    : undefined}
                >
                  {row[side]}
                </div>
              </dd>
            </div>
          {/each}
        </dl>
        <!-- 서로 다른 값이 똑같아 보이면(줄 끝 공백, 보이지 않는 글자 등) 그렇다고 알린다(2차 검증). -->
        {#if row.looksSame}
          <p class="text-muted-foreground" data-clash-looks-same>{m.tasks_clash_looks_same()}</p>
        {/if}
      </li>
    {/each}
  </ul>
  <div class="flex flex-wrap gap-2">
    <Button size="sm" disabled={busy} onclick={onconfirm}>{m.tasks_clash_overwrite()}</Button>
    <Button size="sm" variant="outline" disabled={busy} onclick={oncancel}
      >{m.common_cancel()}</Button
    >
  </div>
</div>
