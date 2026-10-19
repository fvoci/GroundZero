<script lang="ts">
  // 문서 목차(위키 읽기 화면). 편집기 영역에 그려진 본문 제목(h1–h3)을 차례대로 보이고, 누르면 그 제목으로 간다.
  // 그려진 본문을 보므로 일반 편집·공동 편집·미리보기에서 똑같이 동작하고, 본문이 바뀌면(입력·다른 사람의 편집) 따라
  // 바뀐다. 제목이 둘보다 적으면 보이지 않는다.
  import { m } from '$lib/paraglide/messages.js';

  interface Props {
    /** 본문이 그려지는 영역. */
    root: HTMLElement | undefined;
  }

  const { root }: Props = $props();

  interface Heading {
    level: number;
    text: string;
    element: HTMLElement;
  }

  let headings = $state<Heading[]>([]);

  function read(area: HTMLElement): Heading[] {
    return [...area.querySelectorAll<HTMLElement>('h1, h2, h3')]
      .map((element) => ({
        level: Number(element.tagName.slice(1)),
        text: element.textContent.trim(),
        element,
      }))
      .filter((h) => h.text !== '');
  }

  $effect(() => {
    const area = root;
    if (area === undefined) {
      headings = [];
      return;
    }
    headings = read(area);
    const observer = new MutationObserver(() => {
      headings = read(area);
    });
    observer.observe(area, { childList: true, subtree: true, characterData: true });
    return () => {
      observer.disconnect();
    };
  });
</script>

{#if headings.length >= 2}
  <nav aria-label={m.documents_outline()} class="flex flex-col gap-1 text-sm">
    <p class="font-semibold">{m.documents_outline()}</p>
    <ul class="flex flex-col gap-0.5">
      {#each headings as heading, index (index)}
        <li style:padding-left={`${String((heading.level - 1) * 0.75)}rem`}>
          <button
            type="button"
            class="w-full truncate rounded px-1 py-0.5 text-left text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            onclick={() => {
              heading.element.scrollIntoView({ block: 'start' });
            }}>{heading.text}</button
          >
        </li>
      {/each}
    </ul>
  </nav>
{/if}
