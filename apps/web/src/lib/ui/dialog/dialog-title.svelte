<!-- 수정(2026-10-25): 제목이 두 줄 이상이면 오른쪽 위 닫기 단추(dialog-content) 밑으로 글이 들어가 겹쳤다. pr-6로 그 자리를 비운다(Storybook에서 발견, 업무 상세 대화상자에서 재현). -->
<script lang="ts">
  import { Dialog as DialogPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils.js';

  let {
    ref = $bindable(null),
    class: className,
    ...restProps
  }: DialogPrimitive.TitleProps = $props();
</script>

<DialogPrimitive.Title
  bind:ref
  data-slot="dialog-title"
  class={cn('text-base leading-none font-medium pr-6', className)}
  {...restProps}
/>
