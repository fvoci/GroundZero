import { render, screen } from '@testing-library/svelte';
import { beforeEach, describe, expect, it } from 'vitest';

import type { HealthResult } from '$lib/api/client';
import { fetchHealth } from '$lib/api/client';
import { setLocale } from '$lib/paraglide/runtime.js';
import { buildLabel } from '$lib/features/health/build';
import HealthStatus from '../../src/lib/features/health/HealthStatus.svelte';

describe('HealthStatus', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });

  it('renders ok state from loader', async () => {
    const load = (): Promise<HealthResult> =>
      Promise.resolve({
        kind: 'ok',
        health: { status: 'ok', database: 'ok', build: { version: 'dev', source: 'dev' } },
      });
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
      Promise.resolve(
        new Response(
          '{"status":"degraded","database":"error","build":{"version":"dev","source":"dev"}}',
          { status: 503 },
        ),
      );
    const result = await fetchHealth(fakeFetch);
    expect(result.kind).toBe('degraded');
  });
});

// 실행 중인 빌드 표시(D-87): 서버 `/healthz`의 build를 그대로 쓰고, 개발 빌드는 릴리스로 보이지 않게 한다.
describe('build label', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });

  it('shows a release version with the short source SHA and marks dev builds', () => {
    expect(
      buildLabel({ version: '0.0.2', source: '96de6afb3519d01840f1366eac8936c9afd70be1' }),
    ).toBe('fvoci 0.0.2 · 96de6af');
    expect(buildLabel({ version: 'dev', source: 'dev' })).toBe('fvoci 개발 빌드(dev)');
    expect(buildLabel(undefined)).toBe('fvoci 버전 확인 불가');
  });

  it('reads the build from the health response', async () => {
    const body = {
      status: 'degraded',
      database: 'error',
      build: { version: '0.0.2', source: '96de6afb3519d01840f1366eac8936c9afd70be1' },
    };
    const fakeFetch: typeof fetch = () =>
      Promise.resolve(new Response(JSON.stringify(body), { status: 503 }));
    const result = await fetchHealth(fakeFetch);
    expect(result.kind).toBe('degraded');
    if (result.kind !== 'unreachable') expect(result.health.build.version).toBe('0.0.2');
  });
});
