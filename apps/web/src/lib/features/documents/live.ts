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
  return { document, provider, user };
}

export function closeLive(session: LiveSession): void {
  session.provider.destroy();
  session.document.destroy();
}

/** 맞춰짐 알림을 주는 쪽(y-websocket의 provider). */
export interface SyncSource {
  readonly synced: boolean;
  on(event: 'sync', listener: (synced: boolean) => void): void;
  off(event: 'sync', listener: (synced: boolean) => void): void;
}

/** 서버와 맞춰지지 않은 채 이 브라우저에서 고친 것이 있는지 따라간다. */
export interface UnsyncedWatch {
  /** 서버에 아직 닿지 않았을 수 있는 내 편집이 있다. */
  readonly dirty: boolean;
  stop(): void;
}

/**
 * 연결이 끊겼거나 아직 맞춰지지 않은 동안 내가 고치면 `onDirty`, 다시 맞춰지면(서버가 받음) `onSynced`를 부른다.
 * 그 사이의 입력을 초안으로 남겨, 실시간이 꺼지거나 권한이 바뀌거나 새로고침해도 사라지지 않게 하는 데 쓴다.
 * 남이 보낸 변경(origin이 provider)은 세지 않는다.
 */
export function watchUnsynced(
  document: Y.Doc,
  provider: SyncSource,
  handlers: { onDirty: () => void; onSynced: () => void },
): UnsyncedWatch {
  let dirty = false;
  const onUpdate = (_update: Uint8Array, origin: unknown): void => {
    if (origin === provider || provider.synced) return;
    dirty = true;
    handlers.onDirty();
  };
  const onSync = (synced: boolean): void => {
    if (!synced || !dirty) return;
    dirty = false;
    handlers.onSynced();
  };
  document.on('update', onUpdate);
  provider.on('sync', onSync);
  return {
    get dirty() {
      return dirty;
    },
    stop() {
      document.off('update', onUpdate);
      provider.off('sync', onSync);
    },
  };
}
