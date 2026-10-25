// 같은 줄에서 묻는 지우기 확인("지울까요? [지우기][취소]")의 초점. 누른 단추가 사라지면 초점이 문서 처음으로
// 빠지므로, 물을 때는 확인 단추로, 취소하면 처음 단추로 옮긴다(`trash/DeleteButton.svelte`와 같은 방식).
// 단추는 `data-confirm="<이름>"`(처음 단추)·`data-confirm-go="<이름>"`(확인 단추)로 찾는다. 이름은 영문·숫자·`-`만.
import { tick } from 'svelte';

async function focusAfterRender(selector: string): Promise<void> {
  await tick();
  document.querySelector<HTMLElement>(selector)?.focus();
}

/** 확인을 보인 뒤 확인 단추로 초점을 옮긴다. */
export function focusConfirm(name: string): Promise<void> {
  return focusAfterRender(`[data-confirm-go="${name}"]`);
}

/** 확인을 거둔 뒤 처음 단추로 초점을 돌린다. */
export function focusConfirmStart(name: string): Promise<void> {
  return focusAfterRender(`[data-confirm="${name}"]`);
}
