// Storybook story를 jsdom에서 다시 쓴다(composeStories). 모든 story가 오류 없이 그려지고, play 함수(키보드·저장
// 실패·입력 보존 등 상호작용)가 통과하는지 본다. 실제 배치·픽셀은 jsdom이 계산하지 않으므로 Storybook을 실제
// 브라우저로 열어 확인한다(D-108 docs/decisions/ui.md#d-108).
import { readdirSync } from 'node:fs';
import { resolve, sep } from 'node:path';

import { composeStories, setProjectAnnotations } from '@storybook/svelte';
import type { StoryObj, SvelteRenderer } from '@storybook/svelte';
import type { Store_CSFExports } from 'storybook/internal/types';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import * as preview from '../../.storybook/preview';

// Vitest disables CSS assets and otherwise gives ?url an empty string. Supply
// only the asset URL; its load event is modeled below, without mocking KaTeX.
vi.mock('../../src/styles/katex-layout.css?url', () => ({ default: '/assets/katex-layout.css' }));
// Match Storybook's spy-only module registration in this standalone Vitest runner.
vi.mock('$lib/features/documents/live', async (original) => {
  const real = await original<typeof import('$lib/features/documents/live')>();
  return { ...real, openLive: vi.fn(real.openLive) };
});

setProjectAnnotations([preview.default]);

const modules = import.meta.glob<Store_CSFExports<SvelteRenderer> & Record<string, StoryObj>>(
  '../../stories/**/*.stories.ts',
  { eager: true },
);
let restoreMathStyles = (): void => undefined;

beforeEach(() => {
  // jsdom does not fetch stylesheets. Model completion of this asset only;
  // the real renderer import and the story's math assertions still run.
  // Network/style success and failure are checked in katex-storybook.spec.ts.
  const styleAppend = new MutationObserver((records) => {
    for (const record of records)
      for (const node of record.addedNodes) {
        if (
          node instanceof HTMLLinkElement &&
          node.rel === 'stylesheet' &&
          node.href.includes('/katex-layout.css')
        )
          node.dispatchEvent(new Event('load'));
      }
  });
  styleAppend.observe(document.head, { childList: true });
  restoreMathStyles = () => {
    styleAppend.disconnect();
  };
  // jsdom에는 matchMedia가 없다(좁은 화면 판정에 쓴다).
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
  // jsdom에는 크기 관찰도 없다(사이드바 툴팁 위치 계산에 쓴다). 크기를 알리지 않는 빈 관찰자를 둔다.
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe = (): undefined => undefined;
      unobserve = (): undefined => undefined;
      disconnect = (): undefined => undefined;
    },
  );
});

afterEach(() => {
  restoreMathStyles();
  vi.unstubAllGlobals();
  // 모달이 닫히기 전에 끝난 story가 남긴 body 잠금(pointer-events 등)을 지운다. 다음 story가 앞 story를 정리한다.
  document.body.removeAttribute('style');
});

it('finds the story files', () => {
  expect(Object.keys(modules).length).toBeGreaterThanOrEqual(8);
});

// Vitest의 파일 모듈 캐시가 이전 glob 결과를 쓰면 새로 더한 story 파일이 조용히 빠진다(통과로 보임). 디스크의 파일과
// 맞춰 보고, 다르면 `pnpm --filter @fvoci/web exec vitest --clearCache` 뒤 다시 실행한다.
it('runs every story file on disk', () => {
  const onDisk = readdirSync(resolve(import.meta.dirname, '../../stories'), {
    recursive: true,
    encoding: 'utf8',
  })
    .filter((file) => file.endsWith('.stories.ts'))
    .map((file) => file.split(sep).join('/'))
    .sort();
  expect(
    Object.keys(modules)
      .map((path) => path.replace('../../stories/', ''))
      .sort(),
  ).toEqual(onDisk);
});

// 스토리 기준(D-108 개정 2026-10-27, #150): 어려운 조건(375px·어두움·200% 글꼴)은 한 파일에 합친 스토리 하나로만 둔다.
// 밝음·어두움·375px·글꼴을 따로 만든 변형(`Dark`·`Narrow375`·`Zoom200` 등)은 두지 않고, `$lib/ui` 부품은 합친 스토리도
// 만들지 않는다(대비는 theme-contrast 시험, 넘침·잘림·초점은 브라우저 시험이 맡는다). 조건은 스토리가 고른 전역값으로 본다.
describe('story conditions', () => {
  type Condition = 'base' | 'combined' | 'partial';

  function conditionOf(globals: {
    viewport?: unknown;
    theme?: unknown;
    fontScale?: unknown;
  }): Condition {
    const viewport = globals.viewport;
    const viewportValue =
      typeof viewport === 'object' && viewport !== null && 'value' in viewport
        ? String(viewport.value)
        : '';
    const dark = globals.theme === 'dark';
    const large = Number(globals.fontScale ?? 100) > 100;
    const narrow = viewportValue.startsWith('narrow');
    const hard = [dark, large, narrow].filter(Boolean).length;
    if (hard === 0) return 'base';
    const exact = dark && Number(globals.fontScale) === 200 && viewportValue === 'narrow375';
    return exact ? 'combined' : 'partial';
  }

  for (const [path, module] of Object.entries(modules)) {
    const file = path.replace('../../stories/', '');
    it(`${file} has at most one combined-condition story and no partial variants`, () => {
      const kinds = Object.entries(composeStories(module)).map(
        ([name, Story]) => [name, conditionOf(Story.globals)] as const,
      );
      expect(
        kinds.filter(([, kind]) => kind === 'partial').map(([name]) => name),
        'stories that set only some of dark·375px·200% (use the one combined story)',
      ).toEqual([]);
      const combined = kinds.filter(([, kind]) => kind === 'combined').map(([name]) => name);
      expect(combined.length, `combined stories: ${combined.join(', ')}`).toBeLessThanOrEqual(
        file.startsWith('ui/') ? 0 : 1,
      );
    });
  }
});

// `$lib/ui` 부품은 부품마다 기본 스토리 하나만 둔다(D-108 개정 2026-10-27, #150). prop·내용·상태를 달리한 변형은 두지 않고
// 대비는 theme-contrast 시험이, 넘침·잘림·초점은 브라우저 시험(e2e)이 단언한다.
describe('ui part stories', () => {
  for (const [path, module] of Object.entries(modules)) {
    const file = path.replace('../../stories/', '');
    if (!file.startsWith('ui/')) continue;
    it(`${file} has exactly one story, Default`, () => {
      expect(Object.keys(composeStories(module))).toEqual(['Default']);
    });
  }
});

for (const [path, module] of Object.entries(modules)) {
  describe(path.replace('../../stories/', ''), () => {
    for (const [name, Story] of Object.entries(composeStories(module))) {
      it(name, async () => {
        await Story.run();
      });
    }
  });
}

// 도구 막대의 "밝기"(밝게·어둡게)는 앱과 같은 applyTheme으로 <html>에 적용한다. 기본은 밝게.
it('applies the theme global to the page like the app does', async () => {
  const badge = Object.entries(modules).find(([path]) => path.endsWith('ui/Badge.stories.ts'))?.[1];
  if (badge === undefined) throw new Error('no Badge stories');
  const Story = Object.values(composeStories(badge))[0];
  if (Story === undefined) throw new Error('no Badge story');
  await Story.run({ globals: { theme: 'dark' } });
  expect(document.documentElement).toHaveClass('dark');
  await Story.run();
  expect(document.documentElement).not.toHaveClass('dark');
});

// 도구 막대의 "글자 크기"는 문자열('200')로 오지만, 주소(`?globals=fontScale:200`)에서는 Storybook이 숫자(200)로 읽는다.
// 둘 다 같은 배율로 적용되고, 지원하지 않는 값은 기본 100%로 돌아간다(임의 값을 CSS에 넣지 않는다).
it('applies the font scale global whether it arrives as the toolbar string or the URL number', async () => {
  const badge = Object.entries(modules).find(([path]) => path.endsWith('ui/Badge.stories.ts'))?.[1];
  if (badge === undefined) throw new Error('no Badge stories');
  const Story = Object.values(composeStories(badge))[0];
  if (Story === undefined) throw new Error('no Badge story');
  const root = document.documentElement;
  try {
    await Story.run({ globals: { fontScale: 200 } });
    expect(root.style.fontSize).toBe('200%');
    await Story.run({ globals: { fontScale: '150' } });
    expect(root.style.fontSize).toBe('150%');
    await Story.run({ globals: { fontScale: 300 } });
    expect(root.style.fontSize).toBe('100%');
    await Story.run({ globals: { fontScale: '200%' } });
    expect(root.style.fontSize).toBe('100%');
    await Story.run();
    expect(root.style.fontSize).toBe('100%');
  } finally {
    root.style.fontSize = '';
  }
});
