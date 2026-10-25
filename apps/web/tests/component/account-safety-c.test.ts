import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
import { api } from '$lib/api/client';
import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
import DocumentActions from '$lib/features/documents/DocumentActions.svelte';
import { ACCESS_DOCUMENT } from '../../stories/fixtures/access';
import { router } from '$lib/router.svelte';
import {
  accountSafety,
  markAccountSwitch,
  switchAccount,
} from '$lib/features/feedback/feedback.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  accountSafety.switching = false;
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
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
  vi.spyOn(api, 'document').mockResolvedValue(ACCESS_DOCUMENT);
  vi.spyOn(api, 'shares').mockResolvedValue([]);
  vi.spyOn(api, 'publicLinks').mockResolvedValue([]);
  render(DocumentActions, { doc: ACCESS_DOCUMENT, workspaces: [], onchanged: () => undefined });
  render(FeedbackHost, { reload });
  await fireEvent.click(screen.getByRole('button', { name: '공유·설정' }));
  await screen.findByRole('dialog');
  switchAccount();
  await screen.findByTestId('account-safe-state');
  await waitFor(() => {
    expect(screen.getByTestId('account-safe-state').contains(document.activeElement)).toBe(true);
  });
  await fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' });
  expect(screen.getByTestId('account-safe-state')).toBeInTheDocument();
  expect(document.documentElement).toHaveAttribute('data-account-switching');
  expect(reload).toHaveBeenCalledTimes(1);
  expect(router.guard?.(() => undefined)).toBe(false);
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
