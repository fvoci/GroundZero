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

export function openLive(documentId: string, user: LiveUser): LiveSession {
  const document = new Y.Doc();
  const scheme = window.location.protocol === 'https:' ? 'wss' : 'ws';
  const base = `${scheme}://${window.location.host}/api/documents/${encodeURIComponent(documentId)}`;
  // 같은 브라우저의 다른 탭끼리 서버를 거치지 않고 주고받지 않게 한다(모든 변경이 서버 검사를 거친다).
  const provider = new WebsocketProvider(base, 'collab', document, {
    maxBackoffTime: 10_000,
    disableBc: true,
  });
  provider.awareness.setLocalStateField('user', user);
  return { document, provider, user };
}

export function closeLive(session: LiveSession): void {
  session.provider.destroy();
  session.document.destroy();
}
