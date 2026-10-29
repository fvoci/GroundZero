import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import HomeProgress from '$lib/features/home/HomeProgress.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

describe('home progress', () => {
  it('labels course completion as episodes', async () => {
    await setLocale('ko', { reload: false });
    render(HomeProgress, { props: { progress: { kind: 'course', done: 1, total: 2 } } });
    expect(screen.getByText('강의 1/2 완료')).toBeVisible();
  });

  it('keeps each study amount in its own unit', async () => {
    await setLocale('ko', { reload: false });
    render(HomeProgress, {
      props: {
        progress: {
          kind: 'study',
          amounts: [
            { unit: 'pages', done: 5, target: 20 },
            { unit: 'problems', done: 10, target: 10 },
          ],
        },
      },
    });
    expect(screen.getByText('공부 5쪽 / 목표 20쪽')).toBeVisible();
    expect(screen.getByText('공부 10문제 / 목표 10문제')).toBeVisible();
  });
});
