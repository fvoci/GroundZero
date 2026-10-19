// 실시간 공동 편집 연결(D-70). 서버(`/api/documents/{id}/collab`)와 y-websocket 메시지로 동기화한다.
// 닫기 코드: 4400 잘못된 변경, 4403 권한·세션 끝, 4409 문서가 다른 길로 바뀌었거나 상태가 어긋남(새 문서로 다시 연다),
// 4410 실시간 꺼짐(일반 편집으로 돌아간다). 4503 너무 많음·느림과 1012 서버 다시 시작은 y-websocket이 스스로 다시 붙는다.
import { WebsocketProvider } from 'y-websocket';
import * as Y from 'yjs';

export interface LiveUser {
  name: string;
  color: string;
}

export interface LiveSession {
  document: Y.Doc;
  provider: WebsocketProvider;
  user: LiveUser;
  /** 서버가 상태를 DB에 저장했다는 확인(저장한 상태의 스냅숏). */
  persisted: PersistedSource;
}

/** 서버의 저장 확인(y-sync 사용자 정의 메시지 100, 서버 `MSG_PERSISTED`). */
export const MSG_PERSISTED = 100;

/** 저장 확인을 받는 쪽. */
export interface PersistedSource {
  on(listener: (snapshot: Y.Snapshot) => void): void;
  off(listener: (snapshot: Y.Snapshot) => void): void;
}

/** lib0 디코더의 공개 필드(이 앱은 lib0에 직접 기대지 않는다). */
interface ByteDecoder {
  arr: Uint8Array;
  pos: number;
}

function readVarUint(decoder: ByteDecoder): number {
  let value = 0;
  let shift = 0;
  for (;;) {
    const byte = decoder.arr[decoder.pos];
    if (byte === undefined) throw new Error('truncated message');
    decoder.pos += 1;
    value += (byte & 0x7f) * 2 ** shift;
    if (byte < 0x80) return value;
    shift += 7;
    if (shift > 49) throw new Error('varint too long');
  }
}

/** 길이가 앞에 붙은 바이트열(서버의 `write_buf`). */
export function readBuffer(decoder: ByteDecoder): Uint8Array {
  const length = readVarUint(decoder);
  const end = decoder.pos + length;
  if (end > decoder.arr.length) throw new Error('truncated message');
  const bytes = decoder.arr.slice(decoder.pos, end);
  decoder.pos = end;
  return bytes;
}

export type LiveStatus = 'connecting' | 'connected' | 'disconnected';

/** 다시 붙지 않고 멈추는 이유. */
export type LiveStop = 'reload' | 'off' | 'denied' | 'invalid';

export function stopFor(code: number): LiveStop | undefined {
  switch (code) {
    case 4409:
      return 'reload';
    case 4410:
      return 'off';
    case 4403:
      return 'denied';
    case 4400:
      return 'invalid';
    default:
      return undefined;
  }
}

// 커서·이름표 색. 커서 표시가 #rrggbb 형식만 받으므로 흰 글씨와 대비가 충분한 색을 고정해 둔다.
const COLORS = [
  '#1d4ed8',
  '#b91c1c',
  '#047857',
  '#7c3aed',
  '#b45309',
  '#be185d',
  '#0e7490',
  '#4d7c0f',
] as const;

/** 사용자마다 같은 색(커서·이름표). */
export function colorFor(id: string): string {
  let hash = 0;
  for (const ch of id) hash = (hash * 31 + (ch.codePointAt(0) ?? 0)) % 1_000_003;
  return COLORS[hash % COLORS.length] ?? COLORS[0];
}

export function openLive(documentId: string, user: LiveUser, write: boolean): LiveSession {
  const document = new Y.Doc();
  const scheme = window.location.protocol === 'https:' ? 'wss' : 'ws';
  const base = `${scheme}://${window.location.host}/api/documents/${encodeURIComponent(documentId)}`;
  // 편집기로 열면 `write=1`: 그사이 쓸 수 없게 됐으면 서버가 4403으로 닫아 지금 권한으로 다시 열게 한다.
  // 같은 브라우저의 다른 탭끼리 서버를 거치지 않고 주고받지 않게 한다(모든 변경이 서버 검사를 거친다).
  const provider = new WebsocketProvider(base, 'collab', document, {
    maxBackoffTime: 10_000,
    disableBc: true,
    params: write ? { write: '1' } : {},
  });
  provider.awareness.setLocalStateField('user', user);
  const listeners = new Set<(snapshot: Y.Snapshot) => void>();
  provider.messageHandlers[MSG_PERSISTED] = (_encoder, decoder: ByteDecoder) => {
    const snapshot = Y.decodeSnapshot(readBuffer(decoder));
    for (const listener of listeners) listener(snapshot);
  };
  const persisted: PersistedSource = {
    on: (listener) => {
      listeners.add(listener);
    },
    off: (listener) => {
      listeners.delete(listener);
    },
  };
  return { document, provider, user, persisted };
}

export function closeLive(session: LiveSession): void {
  session.provider.destroy();
  session.document.destroy();
}

/** 서버가 저장했다고 확인되지 않은 내 편집이 있는지 따라간다. */
export interface UnsyncedWatch {
  /** 서버가 DB에 저장했다고 아직 확인하지 않은 내 편집이 있다. */
  readonly dirty: boolean;
  stop(): void;
}

/**
 * 내가 고친 변경(origin이 provider가 아닌 것)을 모아 두고, 서버의 저장 확인(저장한 상태의 스냅숏)이 그 변경을 담고
 * 있을 때만 확인된 것으로 본다. 연결이 이어지거나 맞춰진 것만으로는 확인으로 보지 않는다(서버가 그 뒤 거절하거나
 * 저장 전에 멈출 수 있다). 내가 고칠 때마다 `onDirty`, 모아 둔 변경이 모두 확인되면 `onSynced`.
 */
export function watchUnsynced(
  document: Y.Doc,
  origin: unknown,
  persisted: PersistedSource,
  handlers: { onDirty: () => void; onSynced: () => void },
): UnsyncedWatch {
  let pending: Uint8Array[] = [];
  const onUpdate = (update: Uint8Array, from: unknown): void => {
    if (from === origin) return;
    pending.push(update);
    handlers.onDirty();
  };
  const onPersisted = (snapshot: Y.Snapshot): void => {
    if (pending.length === 0) return;
    pending = pending.filter((update) => !Y.snapshotContainsUpdate(snapshot, update));
    if (pending.length === 0) handlers.onSynced();
  };
  document.on('update', onUpdate);
  persisted.on(onPersisted);
  return {
    get dirty() {
      return pending.length > 0;
    },
    stop() {
      document.off('update', onUpdate);
      persisted.off(onPersisted);
    },
  };
}
