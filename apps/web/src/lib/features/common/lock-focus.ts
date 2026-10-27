/**
 * 저장 중 잠금 뒤 키보드 초점 되돌리기(R5 2차 delta 4묶음 확인 필요 1).
 *
 * 저장하는 동안 폼은 `<fieldset disabled>`(또는 입력·단추의 `disabled`)로 잠근다(F-16 이후). 입력 칸에서 Enter로 제출하면
 * 그 칸이 곧바로 비활성이 되어 브라우저가 초점을 body로 옮기고(focus fixup), 잠금이 풀려도 돌려놓지 않는다. 폼이 남는
 * 곳(회차·단위 추가, 초대 등)에서 키보드 사용자가 위치를 잃는다.
 *
 * 잠금 방식은 그대로 두고 문서 전체에서 한 번 처리한다. `disabled`가 붙는 순간 그 안에 있던 초점 요소를 기억하고, 그
 * 요소가 다시 쓸 수 있게 되었을 때 초점이 아직 body에 있고 요소가 문서에 붙어 있으면 되돌린다. 그사이 사용자가 다른 곳에
 * 초점을 두었거나, 폼이 닫히며 요소가 사라졌거나 닫히는 중이면(대화상자가 제 초점 복귀를 한다) 아무것도 하지 않는다.
 *
 * 기억하는 것은 잠금 때문에 빠진 초점뿐이다(D5-F-1). 잠길 때 그 요소에 아직 초점이 있거나, 같은 작업(task)에서 초점이
 * 빠졌을 때만이다. 사용자가 앞 작업에서 이미 떠난 입력(빈 곳 클릭, `blur()`, 마우스 클릭으로 단추에 초점을 주지 않는
 * Safari·macOS Firefox에서 단추 누르기)은 나중에 잠기고 풀려도 끌어오지 않는다.
 */
/** 닫히는 중이거나 숨긴 대화상자. */
const CLOSING = [
  '[role="dialog"][data-state="closed"]',
  '[role="alertdialog"][data-state="closed"]',
  '[inert]',
].join(', ');

export function keepFocusAcrossLocks(doc: Document = document): () => void {
  // 마지막으로 초점을 받은 요소. 브라우저가 비활성이 된 요소에서 초점을 뺄 때 blur를 내는지는 엔진마다 다르므로
  // focusin으로 따라간다.
  let last: HTMLElement | undefined;
  // 이번 작업에서 초점을 잃은 요소. 다음 작업에서 지운다.
  let leaving: HTMLElement | undefined;
  // 잠기며 초점을 잃은 요소. 초점은 한 곳뿐이라 하나만 기억한다.
  let lost: HTMLElement | undefined;

  const bodyFocused = (): boolean => doc.activeElement === null || doc.activeElement === doc.body;

  const onFocusIn = (event: FocusEvent): void => {
    if (!(event.target instanceof HTMLElement)) return;
    last = event.target;
    // 다른 곳에 초점을 두었으면 되돌리지 않는다(되돌린 초점 자신은 같은 요소라 지우지 않는다).
    if (lost !== undefined && lost !== event.target) lost = undefined;
  };

  // 잠긴 동안 누르면(초점이 없는 빈 곳·글을 눌러 focusin이 없어도) 사용자가 손을 댄 것이다. 되돌리지 않는다(R5 B 최종 C-2).
  const onPointerDown = (): void => {
    lost = undefined;
  };

  const onFocusOut = (event: FocusEvent): void => {
    if (!(event.target instanceof HTMLElement)) return;
    const target = event.target;
    leaving = target;
    setTimeout(() => {
      if (leaving === target) leaving = undefined;
    }, 0);
  };

  const observer = new MutationObserver((records) => {
    let released = false;
    for (const record of records) {
      const el = record.target;
      if (!(el instanceof Element)) continue;
      if (el.hasAttribute('disabled')) {
        if (
          last !== undefined &&
          el.contains(last) &&
          last.matches(':disabled') &&
          (doc.activeElement === last || (bodyFocused() && leaving === last))
        )
          lost = last;
      } else if (lost !== undefined && el.contains(lost)) released = true;
    }
    if (!released || lost === undefined) return;
    // 사라졌거나 닫히는 중인 대화상자(성공하면 닫히는 대화상자는 잠금이 풀릴 때 `data-state="closed"`로 닫히는 중이다)
    // 안으로는 되돌리지 않는다. 닫힌 Select·Popover·Tooltip 트리거도 `data-state="closed"`라 대화상자로 좁힌다(C-2).
    if (!lost.isConnected || lost.closest(CLOSING) !== null) {
      lost = undefined;
      return;
    }
    // 겹친 잠금(fieldset 안의 비활성 단추 등)이 아직 남았으면 마지막 잠금이 풀릴 때까지 기다린다.
    if (lost.matches(':disabled')) return;
    const target = lost;
    lost = undefined;
    if (bodyFocused()) target.focus({ preventScroll: true });
  });

  doc.addEventListener('focusin', onFocusIn, true);
  doc.addEventListener('focusout', onFocusOut, true);
  doc.addEventListener('pointerdown', onPointerDown, true);
  observer.observe(doc.documentElement, {
    attributes: true,
    attributeFilter: ['disabled'],
    subtree: true,
  });
  return () => {
    doc.removeEventListener('focusin', onFocusIn, true);
    doc.removeEventListener('focusout', onFocusOut, true);
    doc.removeEventListener('pointerdown', onPointerDown, true);
    observer.disconnect();
  };
}
