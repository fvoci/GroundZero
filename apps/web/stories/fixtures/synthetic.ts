// 개발 seed와 같은 합성 데이터(deploy/postgres/dev-ui-data.json)를 화면 타입으로 옮긴다. 내용(제목·본문·트리·
// 스레드·상태)은 `cargo xtask dev`의 "UI 상태 예제" 워크스페이스와 같다. id·시각은 DB가 정하므로 여기서는 고정값을 쓴다
// (시각은 기준 시각에서 몇 분씩 뒤, 표시 시간대는 preview.ts가 Asia/Seoul로 고정).
import type { CommentView, SummaryView } from '$lib/api/client';

import data from '../../../../deploy/postgres/dev-ui-data.json';

export const SAMPLES = data.samples;

/** 합성 데이터가 처음 만들어진 것으로 보는 시각. */
const BASE = Date.parse('2026-10-20T01:30:00Z');

function at(minutes: number): string {
  return new Date(BASE + minutes * 60_000).toISOString();
}

/** 사람 열쇠(owner·writer)의 표시 이름. 개발 Keycloak 테스트 사용자의 표시 이름과 같다. */
export function personName(key: string): string {
  return data.people.find((p) => p.key === key)?.name ?? key;
}

/** 문서 트리: 개발 seed가 같은 순서·부모로 만드는 위키 문서. */
export const DOCS: SummaryView[] = data.documents.map((d, i) => ({
  id: `doc-${d.key}`,
  reference: `WIKI-${String(i + 1)}`,
  title: d.title,
  revision: 1,
  updated_at: at(i),
  parent_id: d.parent === null ? null : `doc-${d.parent}`,
}));

/** 댓글 스레드를 owner(Test User, 워크스페이스 소유자)가 보는 화면의 목록으로 편다. 자기 댓글만 고치고, 지우기는 모두 된다(실제 앱과 같음). */
export function commentThreads(): CommentView[] {
  const list: CommentView[] = [];
  let minute = 0;
  data.comments.threads.forEach((thread, t) => {
    const rootId = `c-${String(t + 1)}`;
    const view = (id: string, parent: string | null, author: string, body: string): CommentView => {
      minute += 7;
      return {
        id,
        parent_id: parent,
        body,
        author_id: `u-${author}`,
        author_name: personName(author),
        created_at: at(minute),
        edited: false,
        deleted: false,
        resolved: parent === null && thread.resolved,
        can_edit: author === 'owner',
        can_delete: true,
        can_resolve: parent === null,
      };
    };
    list.push(view(rootId, null, thread.author, thread.body));
    thread.replies.forEach((reply, r) => {
      list.push(view(`${rootId}-${String(r + 1)}`, rootId, reply.author, reply.body));
    });
  });
  return list;
}

/** 업무 합성 데이터(충돌 확인의 서버 값 등에 쓴다). */
export const TASKS = data.tasks;
