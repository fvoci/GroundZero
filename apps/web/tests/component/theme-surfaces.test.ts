import { cleanup, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
import { connection, notifyDone } from '$lib/features/feedback/feedback.svelte';
import { applyTheme } from '$lib/theme.svelte';

// jsdom에는 matchMedia가 없다(svelte-sonner가 테마를 정할 때 부른다). 실제 브라우저처럼 밝은 시스템 설정으로 둔다.
beforeEach(() => {
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    addListener: () => undefined,
    removeListener: () => undefined,
  }));
});

// 어두운 화면에서 토큰 밖 고정 색을 쓰던 곳: 알림(sonner)은 light로 고정돼 있었고, 노란 안내 띠·상자에는 어두운 짝이 없었다.
afterEach(() => {
  // 테마를 되돌리면 알림이 다시 그려지며 matchMedia를 부르므로, 먼저 내리고 되돌린다.
  cleanup();
  connection.lost = false;
  applyTheme('light');
  vi.unstubAllGlobals();
});

describe('surfaces outside the color tokens in the dark theme', () => {
  it('draws toasts in the current theme', async () => {
    applyTheme('dark');
    render(FeedbackHost);
    notifyDone('저장했습니다.');
    await screen.findByText('저장했습니다.');
    await waitFor(() => {
      expect(document.querySelector('[data-sonner-theme]')).toHaveAttribute(
        'data-sonner-theme',
        'dark',
      );
    });
  });

  it('uses theme tokens for the connection banner', async () => {
    applyTheme('dark');
    vi.stubGlobal('fetch', () => new Promise(() => undefined));
    render(FeedbackHost);
    connection.lost = true;
    const banner = await screen.findByTestId('connection-banner');
    expect(banner).toHaveClass('bg-secondary', 'text-secondary-foreground');
  });
});
