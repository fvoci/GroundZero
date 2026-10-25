import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
import { api } from '$lib/api/client';
import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
import {
  accountSafety,
  markAccountSwitch,
  switchAccount,
} from '$lib/features/feedback/feedback.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  accountSafety.switching = false;
  vi.unstubAllGlobals();
});

it('keeps the safe state when reload cannot complete, and refuses further API requests', async () => {
  await setLocale('ko', { reload: false });
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
  const reload = vi.fn();
  const fetch = vi.fn();
  vi.stubGlobal('fetch', fetch);
  render(FeedbackHost, { reload });
  switchAccount();
  await screen.findByTestId('account-safe-state');
  expect(document.documentElement).toHaveAttribute('data-account-switching');
  expect(reload).toHaveBeenCalledTimes(1);
  await expect(api.workspaces()).rejects.toMatchObject({ code: 'account_changed' });
  expect(fetch).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole('button', { name: '지금 계정으로 다시 읽기' }));
  expect(reload).toHaveBeenCalledTimes(2);
});

it('drops an in-flight response after the account changes', async () => {
  let finish: (response: Response) => void = () => undefined;
  vi.stubGlobal(
    'fetch',
    () =>
      new Promise<Response>((resolve) => {
        finish = resolve;
      }),
  );
  const request = api.workspaces();
  const rejected = expect(request).rejects.toMatchObject({ code: 'account_changed' });
  markAccountSwitch();
  finish(new Response('[]', { status: 200 }));
  await rejected;
  await waitFor(() => {
    expect(accountSafety.switching).toBe(true);
  });
});
