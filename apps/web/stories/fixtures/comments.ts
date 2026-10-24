// 댓글 합성 데이터. 스레드는 개발 seed와 같은 내용(synthetic.ts), 한 댓글의 변형은 `comment()`로 만든다.
import type { CommentView } from '$lib/api/client';

import { commentThreads, personName } from './synthetic';

const THREADS = commentThreads();

/** 첫 스레드의 첫 댓글(owner가 쓴 것)에서 바꾼 한 댓글. */
export function comment(overrides: Partial<CommentView> = {}): CommentView {
  const first = THREADS[0];
  if (first === undefined) throw new Error('synthetic comments are empty');
  return { ...first, ...overrides };
}

export const THREAD: CommentView[] = THREADS;

export const WRITER_NAME = personName('writer');
