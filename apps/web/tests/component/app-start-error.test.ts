import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { connection } from '$lib/features/feedback/feedback.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

import App from '../../src/app/App.svelte';

// 앱 첫 화면이 서버에 닿지 못하면(감사 W33) 브라우저 원문('Failed to fetch') 대신 번역된 문구와 다시 시도 단추를
// 보인다. 다시 시도하면 처음부터 읽는다.
function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

describe('app start-up error', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    window.history.replaceState(null, '', '/');
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
    vi.restoreAllMocks();
    connection.lost = false;
  });

  function serverDownOnce(): void {
    const fetchMock = vi.fn((input: RequestInfo | URL): Promise<Response> => {
      const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
      if (url === '/api/me') return Promise.resolve(json(401, { error: 'unauthorized' }));
      if (url === '/api/auth-info') return Promise.resolve(json(200, { oidc_enabled: true }));
      return Promise.resolve(json(404, { error: 'not_found' }));
    });
    fetchMock.mockRejectedValueOnce(new TypeError('Failed to fetch'));
    vi.stubGlobal('fetch', fetchMock);
  }

  it('shows a translated message with a retry when the server cannot be reached', async () => {
    serverDownOnce();
    render(App);
    const alert = await screen.findByRole('alert');
    expect(alert).not.toHaveTextContent('Failed to fetch');
    expect(alert).toHaveTextContent('서버에 연결할 수 없습니다.');

    await fireEvent.click(screen.getByRole('button', { name: '다시 시도' }));
    // 다시 읽어 로그인 화면으로 간다.
    expect(await screen.findByText(/계속하려면 로그인하세요/)).toBeInTheDocument();
  });

  it('loads again by itself once the connection banner clears', async () => {
    serverDownOnce();
    render(App);
    await screen.findByRole('alert');
    expect(connection.lost).toBe(true);
    // 연결 확인(/healthz)이 다시 닿아 배너가 내려간 것과 같다.
    connection.lost = false;
    expect(await screen.findByText(/계속하려면 로그인하세요/)).toBeInTheDocument();
  });
});
