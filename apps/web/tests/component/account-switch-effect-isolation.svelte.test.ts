import { flushSync } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import { accountSafety } from '$lib/features/feedback/feedback.svelte';

// #88·D-96: 화면의 $effect가 API를 부를 때 request()가 읽는 전환 상태가 그 효과의 의존성이 되면, 계정 전환이 시작되는
// 순간 항목·업무 화면의 읽기 효과가 다시 실행돼 로딩 상태로 돌아가고 DOM(메모리의 입력)과 떠나기 가드가 사라진다(실제
// 브라우저 진단으로 찾음). 전환 상태 읽기는 반응형이 아니어야 하고, 전환 중 요청 차단은 그대로여야 한다.

function meResponse(): Response {
  return new Response(
    JSON.stringify({
      id: 'user-a',
      email: 'a@example.com',
      display_name: 'A',
      is_server_admin: false,
      time_zone: '',
      locale: '',
      week_start: '',
      hour_cycle: '',
      theme: 'system',
      limits: {
        max_file_bytes: 1,
        max_storage_bytes: 1,
        max_concurrent_uploads: 1,
        max_note_bytes: 1,
      },
    }),
    { status: 200, headers: { 'Content-Type': 'application/json' } },
  );
}

describe('effects that call the API do not re-run when an account switch starts', () => {
  let stop: () => void = () => undefined;
  beforeEach(() => {
    accountSafety.switching = false;
  });
  afterEach(() => {
    stop();
    accountSafety.switching = false;
    vi.restoreAllMocks();
  });

  it('keeps the loading effect as it was and still blocks the request made during the switch', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(() => Promise.resolve(meResponse()));
    let runs = 0;
    const results: string[] = [];
    stop = $effect.root(() => {
      $effect(() => {
        runs += 1;
        api
          .me()
          .then((me) => {
            results.push(me === undefined ? 'none' : me.id);
          })
          .catch((e: unknown) => {
            results.push(e instanceof Error ? e.message : String(e));
          });
      });
    });
    flushSync();
    expect(runs).toBe(1);
    await vi.waitFor(() => {
      expect(results).toHaveLength(1);
    });

    accountSafety.switching = true;
    flushSync();
    // 전환이 시작돼도 읽기 효과는 다시 돌지 않는다(화면·입력·가드가 남는다).
    expect(runs).toBe(1);
    // 전환 중의 새 요청은 전처럼 막힌다.
    await expect(api.me()).rejects.toMatchObject({ code: 'account_changed' });
  });
});
