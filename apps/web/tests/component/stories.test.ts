// Storybook story를 jsdom에서 다시 쓴다(composeStories). 모든 story가 오류 없이 그려지고, play 함수(키보드·저장
// 실패·입력 보존 등 상호작용)가 통과하는지 본다. 실제 배치·픽셀은 jsdom이 계산하지 않으므로 Storybook을 실제
// 브라우저로 열어 확인한다(docs/development.md의 Storybook 절).
import { composeStories, setProjectAnnotations } from '@storybook/svelte';
import type { StoryObj, SvelteRenderer } from '@storybook/svelte';
import type { Store_CSFExports } from 'storybook/internal/types';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import * as preview from '../../.storybook/preview';

setProjectAnnotations([preview.default]);

const modules = import.meta.glob<Store_CSFExports<SvelteRenderer> & Record<string, StoryObj>>(
  '../../stories/**/*.stories.ts',
  { eager: true },
);

beforeEach(() => {
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
  vi.unstubAllGlobals();
  // 모달이 닫히기 전에 끝난 story가 남긴 body 잠금(pointer-events 등)을 지운다. 다음 story가 앞 story를 정리한다.
  document.body.removeAttribute('style');
});

it('finds the story files', () => {
  expect(Object.keys(modules).length).toBeGreaterThanOrEqual(8);
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
