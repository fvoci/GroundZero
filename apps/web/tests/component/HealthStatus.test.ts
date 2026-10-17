import { render, screen } from '@testing-library/svelte';
import { beforeEach, describe, expect, it } from 'vitest';

import type { HealthResult } from '$lib/api/client';
import { fetchHealth } from '$lib/api/client';
import { setLocale } from '$lib/paraglide/runtime.js';
import HealthStatus from '../../src/lib/features/health/HealthStatus.svelte';

describe('HealthStatus', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });

  it('renders ok state from loader', async () => {
    const load = (): Promise<HealthResult> =>
      Promise.resolve({ kind: 'ok', health: { status: 'ok', database: 'ok' } });
    render(HealthStatus, { load });
    expect(await screen.findByTestId('health')).toHaveTextContent('서버 정상');
  });

  it('renders unreachable state when loader fails', async () => {
    const load = (): Promise<HealthResult> => Promise.reject(new Error('boom'));
    render(HealthStatus, { load });
    expect(await screen.findByTestId('health')).toHaveTextContent('boom');
  });
});

describe('fetchHealth', () => {
  it('treats malformed body as unreachable', async () => {
    const fakeFetch: typeof fetch = () =>
      Promise.resolve(new Response('{"nope":1}', { status: 200 }));
    const result = await fetchHealth(fakeFetch);
    expect(result.kind).toBe('unreachable');
  });

  it('maps 503 with valid body to degraded', async () => {
    const fakeFetch: typeof fetch = () =>
      Promise.resolve(new Response('{"status":"degraded","database":"error"}', { status: 503 }));
    const result = await fetchHealth(fakeFetch);
    expect(result.kind).toBe('degraded');
  });
});
