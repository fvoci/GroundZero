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
