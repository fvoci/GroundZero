import { type CommentView } from '$lib/features/activity/api';
// 댓글 목록을 스레드(첫 댓글 + 답글)로 묶는다. 서버는 스레드 순으로 주지만 순서에 기대지 않는다.

export interface Thread {
  root: CommentView;
  replies: CommentView[];
}

export function threads(comments: readonly CommentView[]): Thread[] {
  const roots = comments.filter((c) => c.parent_id === null);
  return roots.map((root) => ({
    root,
    replies: comments
      .filter((c) => c.parent_id === root.id)
      .sort((a, b) => a.created_at.localeCompare(b.created_at)),
  }));
}
