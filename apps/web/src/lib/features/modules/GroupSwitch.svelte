<script lang="ts" generics="T extends string">
  // 항목 묶음·모듈 보기 전환(D-121): 하위 경로마다 링크 하나인 단추 묶음. 좁은 화면에서는 줄바꿈한다(선택 상자로 바꾸지
  // 않는다). 고른 것은 aria-current="page"다. 링크라 새 탭·주소 복사도 되고, 보통 누르기는 앱 안 이동(pushState)이다.
  interface Option {
    id: T;
    label: string;
    href: string;
  }

  interface Props {
    label: string;
    options: readonly Option[];
    current: T | undefined;
    onselect: (id: T) => void;
  }

  const { label, options, current, onselect }: Props = $props();
</script>

<nav aria-label={label}>
  <ul class="flex flex-wrap gap-1">
    {#each options as option (option.id)}
      {@const active = option.id === current}
      <li>
        <a
          class={[
            'inline-flex min-h-9 items-center rounded-md px-3 text-sm font-medium focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none',
            active
              ? 'bg-secondary text-secondary-foreground'
              : 'text-muted-foreground hover:bg-accent hover:text-foreground',
          ]}
          href={option.href}
          aria-current={active ? 'page' : undefined}
          onclick={(e: MouseEvent) => {
            if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey || e.button !== 0) return;
            e.preventDefault();
            onselect(option.id);
          }}>{option.label}</a
        >
      </li>
    {/each}
  </ul>
</nav>
