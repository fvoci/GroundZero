// 배치 진단: 상자 밖으로 삐져나온 요소를 찾는다(실패 메시지에 담아 어느 요소가 넘치는지 바로 보이게).
import type { Locator } from '@playwright/test';

export interface Offender {
  tag: string;
  cls: string;
  text: string;
  right: number;
  limit: number;
}

/** `container` 안에서 오른쪽 끝이 상자의 안쪽 폭을 넘는 요소(넘친 폭 큰 순, 최대 8개). */
export async function offenders(container: Locator): Promise<Offender[]> {
  return container.evaluate((root) => {
    const limit = root.getBoundingClientRect().left + root.clientWidth;
    const found: { tag: string; cls: string; text: string; right: number; limit: number }[] = [];
    for (const el of root.querySelectorAll<HTMLElement>('*')) {
      const r = el.getBoundingClientRect();
      if (r.width > 0 && r.right > limit + 1)
        found.push({
          tag: el.tagName.toLowerCase(),
          cls: el.className.slice(0, 80),
          text: el.innerText.trim().slice(0, 40),
          right: Math.round(r.right),
          limit: Math.round(limit),
        });
    }
    return found.sort((a, b) => b.right - a.right).slice(0, 8);
  });
}
