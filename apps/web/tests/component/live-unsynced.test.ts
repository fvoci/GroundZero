import { describe, expect, it } from 'vitest';
import * as Y from 'yjs';

import { watchUnsynced } from '$lib/features/documents/live';

/** y-websocket의 provider 흉내: synced 값과 'sync' 알림만. */
function fakeProvider() {
  const listeners = new Set<(synced: boolean) => void>();
  return {
    synced: true,
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

// 끊긴 동안의 실시간 입력을 초안으로 남기는 근거(제품 로직 점검 1번 불변식).
describe('watchUnsynced', () => {
  it('marks edits made while not synced and clears once synced again', () => {
    const doc = new Y.Doc();
    const provider = fakeProvider();
    const events: string[] = [];
    const watch = watchUnsynced(doc, provider, {
      onDirty: () => events.push('dirty'),
      onSynced: () => events.push('synced'),
    });

    doc.getText('t').insert(0, '연결됨');
    expect(watch.dirty).toBe(false);

    provider.setSynced(false);
    doc.getText('t').insert(0, '끊긴 동안');
    expect(watch.dirty).toBe(true);

    // 남이 보낸 변경은 내 편집이 아니다.
    const remote = new Y.Doc();
    remote.getText('t').insert(0, '남');
    const other = fakeProvider();
    const watchOther = watchUnsynced(remote, other, {
      onDirty: () => events.push('other-dirty'),
      onSynced: () => undefined,
    });
    other.synced = false;
    Y.applyUpdate(remote, Y.encodeStateAsUpdate(doc), other);
    expect(watchOther.dirty).toBe(false);

    provider.setSynced(true);
    expect(watch.dirty).toBe(false);
    expect(events).toEqual(['dirty', 'synced']);
    watch.stop();
    watchOther.stop();
  });
});
