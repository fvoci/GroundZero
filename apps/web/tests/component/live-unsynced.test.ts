import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import * as Y from 'yjs';

import { CONFIRM_MS, watchUnsynced } from '$lib/features/documents/live';

/** y-websocket의 provider 흉내: synced·wsconnected 값과 'sync' 알림만. */
function fakeProvider() {
  const listeners = new Set<(synced: boolean) => void>();
  return {
    synced: true,
    wsconnected: true,
    on(_event: 'sync', listener: (synced: boolean) => void) {
      listeners.add(listener);
    },
    off(_event: 'sync', listener: (synced: boolean) => void) {
      listeners.delete(listener);
    },
    setSynced(value: boolean) {
      this.synced = value;
      for (const l of listeners) l(value);
    },
  };
}

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

// 서버가 받았다고 확인될 때까지 내 실시간 입력을 초안으로 남긴다(제품 로직 점검 1번 불변식).
describe('watchUnsynced', () => {
  it('confirms an edit only after the connection stays synced for a while', () => {
    const doc = new Y.Doc();
    const provider = fakeProvider();
    const events: string[] = [];
    const watch = watchUnsynced(doc, provider, {
      onDirty: () => events.push('dirty'),
      onSynced: () => events.push('confirmed'),
    });

    doc.getText('t').insert(0, '연결됨');
    expect(watch.dirty).toBe(true);
    vi.advanceTimersByTime(CONFIRM_MS);
    expect(watch.dirty).toBe(false);
    expect(events).toEqual(['dirty', 'confirmed']);
    watch.stop();
  });

  it('keeps an offline edit when the reconnect is rejected right after syncing', () => {
    const doc = new Y.Doc();
    const provider = fakeProvider();
    const watch = watchUnsynced(doc, provider, {
      onDirty: () => undefined,
      onSynced: () => undefined,
    });

    provider.wsconnected = false;
    provider.setSynced(false);
    doc.getText('t').insert(0, '끊긴 동안');
    vi.advanceTimersByTime(CONFIRM_MS * 2);
    expect(watch.dirty).toBe(true);

    // 다시 붙어 서버 상태가 도착했다(synced). 서버는 곧 내 편집을 거절하고 닫는다(4409·4400).
    provider.wsconnected = true;
    provider.setSynced(true);
    vi.advanceTimersByTime(100);
    provider.wsconnected = false;
    provider.setSynced(false);
    vi.advanceTimersByTime(CONFIRM_MS * 2);
    expect(watch.dirty).toBe(true);
    watch.stop();
  });

  it('does not count changes sent by others', () => {
    const doc = new Y.Doc();
    doc.getText('t').insert(0, '남');
    const remote = new Y.Doc();
    const provider = fakeProvider();
    const watch = watchUnsynced(remote, provider, {
      onDirty: () => undefined,
      onSynced: () => undefined,
    });
    Y.applyUpdate(remote, Y.encodeStateAsUpdate(doc), provider);
    expect(watch.dirty).toBe(false);
    watch.stop();
  });
});
