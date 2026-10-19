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
  readonly wsconnected: boolean;
  on(event: 'sync', listener: (synced: boolean) => void): void;
  off(event: 'sync', listener: (synced: boolean) => void): void;
}

/** 서버가 받았다고 확인되지 않은 내 편집이 있는지 따라간다. */
export interface UnsyncedWatch {
  /** 서버가 받았다고 아직 확인하지 못한 내 편집이 있다. */
  readonly dirty: boolean;
  stop(): void;
}

/**
 * 서버가 받았다고 볼 때까지 기다리는 시간. `synced`는 서버의 상태가 도착했다는 뜻일 뿐이고, 서버는 내 편집을
 * 그 뒤(최대 0.1초) 검사해 거절(4400)하거나 다시 열게(4409) 할 수 있다. 그동안 연결이 이어지고 맞춰져 있으면 받은
 * 것으로 본다.
 */
export const CONFIRM_MS = 2000;

/**
 * 내가 고칠 때마다 `onDirty`, 그 뒤 `CONFIRM_MS` 동안 연결이 이어지고 맞춰져 있으면 `onSynced`를 부른다(끊기면
 * 다시 맞춰진 뒤부터 센다). 그 사이의 입력을 초안으로 남겨, 끊김·거절·실시간 꺼짐·권한 변경·새로고침에도 사라지지
 * 않게 하는 데 쓴다. 남이 보낸 변경(origin이 provider)은 세지 않는다.
 */
export function watchUnsynced(
  document: Y.Doc,
  provider: SyncSource,
  handlers: { onDirty: () => void; onSynced: () => void },
): UnsyncedWatch {
  let dirty = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const cancel = (): void => {
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
  };
  const schedule = (): void => {
    cancel();
    if (!dirty || !provider.synced) return;
    timer = setTimeout(() => {
      timer = undefined;
      if (!dirty || !provider.synced || !provider.wsconnected) return;
      dirty = false;
      handlers.onSynced();
    }, CONFIRM_MS);
  };
  const onUpdate = (_update: Uint8Array, origin: unknown): void => {
    if (origin === provider) return;
    dirty = true;
    handlers.onDirty();
    schedule();
  };
  const onSync = (synced: boolean): void => {
    if (synced) schedule();
    else cancel();
  };
  document.on('update', onUpdate);
  provider.on('sync', onSync);
  return {
    get dirty() {
      return dirty;
    },
    stop() {
      cancel();
      document.off('update', onUpdate);
      provider.off('sync', onSync);
    },
  };
}
