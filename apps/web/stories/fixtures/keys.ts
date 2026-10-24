import { fireEvent } from 'storybook/test';

/**
 * 지금 초점이 있는 요소에 Escape를 보낸다. Bits UI는 Escape 이벤트를 복제해 처리하는데, jsdom에서 userEvent가 만든
 * 키 이벤트는 `view`가 jsdom의 창이 아니라 복제에 실패한다. 실제 브라우저와 jsdom 모두에서 같은 경로로 처리되도록
 * 창을 지정하지 않는 키 이벤트를 쓴다.
 */
export async function pressEscape(): Promise<void> {
  const target = document.activeElement ?? document.body;
  await fireEvent.keyDown(target, { key: 'Escape', code: 'Escape' });
  await fireEvent.keyUp(target, { key: 'Escape', code: 'Escape' });
}
