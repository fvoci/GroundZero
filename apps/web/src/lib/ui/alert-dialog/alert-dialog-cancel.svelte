<script lang="ts">
  // 손댐(2026-10-22): 생성기는 `buttonVariants(...)`로 클래스를 만들었는데, ESLint가 .svelte 모듈에서 내보낸 함수의
  // 타입을 해석하지 못해(no-unsafe-call) 거부한다. dialog-content의 닫기 단추처럼 `child`로 `Button`을 그린다(겉모습 같음).
  import { AlertDialog as AlertDialogPrimitive } from 'bits-ui';
  import { Button, type ButtonVariant, type ButtonSize } from '$lib/ui/button/index.js';
  import type { WithoutChild } from '$lib/utils.js';

  let {
    ref = $bindable(null),
    class: className,
    variant = 'outline',
    size = 'default',
    children,
    ...restProps
  }: WithoutChild<AlertDialogPrimitive.CancelProps> & {
    variant?: ButtonVariant;
    size?: ButtonSize;
  } = $props();
</script>

<AlertDialogPrimitive.Cancel bind:ref data-slot="alert-dialog-cancel" {...restProps}>
  {#snippet child({ props })}
    <Button {...props} {variant} {size} class={className}>{@render children?.()}</Button>
  {/snippet}
</AlertDialogPrimitive.Cancel>
