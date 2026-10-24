// 실제 배치 결과로 한국어 줄바꿈과 가로 넘침을 잰다. jsdom 정적 검사(tests/korean-line-break.test.ts)가 보지 못하는 부분이다.
//
// 한글이 든 어절(공백으로 나눈 덩어리)마다 Range의 줄 상자를 세어 두 줄 이상이면 어절 중간에서 끊긴 것이다.
// 단, 어절 자체가 담는 요소보다 넓으면 끊김은 피할 수 없으므로(overflow-wrap: anywhere) 따로 센다.
import { type Locator, type Page, expect } from '@playwright/test';

export interface Wrap {
  /** 담는 요소보다 좁은데도 두 줄 이상에 걸친 어절(회귀). */
  broken: string[];
  /** 담는 요소보다 넓어 끊길 수밖에 없는 어절. */
  unavoidable: string[];
  /** 검사한 한글 어절 수. */
  words: number;
  /** 요소 글자가 차지한 줄 수. */
  lines: number;
  /** 요소 안에서 가로로 넘쳤는지. */
  overflow: boolean;
}

/** 서체(앱이 내보내는 Noto Sans KR 등)를 다 읽은 뒤 잰다. 읽는 중의 배치를 회귀로 오인하지 않는다. */
export async function measureWrap(target: Locator): Promise<Wrap> {
  await target.page().evaluate(async () => {
    await document.fonts.ready;
  });
  return target.evaluate((root) => {
    // 같은 줄의 상자는 위치가 조금 달라도(글꼴 대체) 높이의 절반 안에 있다.
    const lineCount = (rects: DOMRect[]): number => {
      let lines = 0;
      let top = -Infinity;
      let height = 0;
      for (const r of rects
        .filter((r) => r.width > 0 && r.height > 0)
        .sort((a, b) => a.top - b.top)) {
        if (r.top >= top + height / 2) {
          lines += 1;
          top = r.top;
          height = r.height;
        }
      }
      return lines;
    };
    const hangul = /[가-힣]/;
    const broken: string[] = [];
    const unavoidable: string[] = [];
    let words = 0;
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node !== null; node = walker.nextNode()) {
      for (const match of (node.textContent ?? '').matchAll(/\S+/g)) {
        if (!hangul.test(match[0])) continue;
        words += 1;
        const range = document.createRange();
        range.setStart(node, match.index);
        range.setEnd(node, match.index + match[0].length);
        const rects = [...range.getClientRects()];
        if (lineCount(rects) < 2) continue;
        const width = rects.reduce((sum, r) => sum + r.width, 0);
        (width > root.clientWidth ? unavoidable : broken).push(match[0]);
      }
    }
    const all = document.createRange();
    all.selectNodeContents(root);
    return {
      broken,
      unavoidable,
      words,
      lines: lineCount([...all.getClientRects()]),
      overflow: root.scrollWidth > root.clientWidth + 1,
    };
  });
}

/** 문서 전체가 가로로 넘치는지(가로 스크롤이 생기는지). */
export async function pageOverflows(page: Page): Promise<boolean> {
  return page.evaluate(
    () => document.documentElement.scrollWidth > document.documentElement.clientWidth,
  );
}

/** 한국어 어절을 끊지 않고, 넘치지 않으며, 좁은 화면에서는 실제로 여러 줄로 배치됐는지(헛검사 방지). */
export async function expectKoreanWrap(target: Locator, minLines: number): Promise<Wrap> {
  const wrap = await measureWrap(target);
  expect(wrap.words, 'Korean words measured').toBeGreaterThan(0);
  expect(wrap.broken, 'Korean words split in the middle').toEqual([]);
  expect(wrap.lines, 'lines laid out').toBeGreaterThanOrEqual(minLines);
  expect(wrap.overflow, 'element overflows horizontally').toBe(false);
  expect(await pageOverflows(target.page()), 'page scrolls horizontally').toBe(false);
  return wrap;
}

/** 설정한 화면 폭이 실제로 적용됐는지(viewport 시험의 직접 증거). */
export async function expectViewport(page: Page, narrow: boolean): Promise<void> {
  expect(await page.evaluate(() => window.innerWidth)).toBe(narrow ? 375 : 1280);
}
