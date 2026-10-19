import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import * as Y from 'yjs';

import type { PersistedSource } from '$lib/features/documents/live';
import { watchUnsynced } from '$lib/features/documents/live';

/** 서버의 저장 확인을 흉내 낸다. */
function persistedSource() {
  const listeners = new Set<(snapshot: Y.Snapshot) => void>();
  const source: PersistedSource & { confirm(doc: Y.Doc): void } = {
    on: (listener) => {
      listeners.add(listener);
    },
    off: (listener) => {
      listeners.delete(listener);
    },
    /** 서버가 `doc`의 지금 상태를 저장했다고 알린다. */
    confirm(doc: Y.Doc) {
      const snapshot = Y.snapshot(doc);
      for (const l of listeners) l(snapshot);
    },
  };
  return source;
}

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

// 서버가 DB에 저장했다고 확인할 때까지 내 실시간 입력을 초안으로 남긴다(제품 로직 점검 1번 불변식, 마무리 ①).
describe('watchUnsynced', () => {
  it('stays unconfirmed without a persisted acknowledgement, however long it waits', () => {
    const doc = new Y.Doc();
    const provider = {};
    const persisted = persistedSource();
    const watch = watchUnsynced(doc, provider, persisted, {
      onDirty: () => undefined,
      onSynced: () => undefined,
    });
    doc.getText('t').insert(0, '연결됨');
    vi.advanceTimersByTime(60_000);
    expect(watch.dirty).toBe(true);
    watch.stop();
  });

  it('is confirmed only by an acknowledgement that contains the edit', () => {
    const doc = new Y.Doc();
    const provider = {};
    const persisted = persistedSource();
    const events: string[] = [];
    const watch = watchUnsynced(doc, provider, persisted, {
      onDirty: () => events.push('dirty'),
      onSynced: () => events.push('confirmed'),
    });
    // 서버가 저장한 것은 내 편집 전의 상태다.
    const server = new Y.Doc();
    doc.getText('t').insert(0, '내 편집');
    persisted.confirm(server);
    expect(watch.dirty).toBe(true);
    // 지움도 저장 확인에 들어 있어야 한다.
    Y.applyUpdate(server, Y.encodeStateAsUpdate(doc));
    doc.getText('t').delete(0, 1);
    persisted.confirm(server);
    expect(watch.dirty).toBe(true);
    Y.applyUpdate(server, Y.encodeStateAsUpdate(doc));
    persisted.confirm(server);
    expect(watch.dirty).toBe(false);
    expect(events).toEqual(['dirty', 'dirty', 'confirmed']);
    watch.stop();
  });

  it('does not count changes sent by others', () => {
    const other = new Y.Doc();
    other.getText('t').insert(0, '남');
    const doc = new Y.Doc();
    const provider = {};
    const watch = watchUnsynced(doc, provider, persistedSource(), {
      onDirty: () => undefined,
      onSynced: () => undefined,
    });
    Y.applyUpdate(doc, Y.encodeStateAsUpdate(other), provider);
    expect(watch.dirty).toBe(false);
    watch.stop();
  });
});
