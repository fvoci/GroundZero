// 댓글 합성 데이터. 시각·id는 고정한다(표시 시간대는 preview.ts가 Asia/Seoul로 고정).
import type { CommentView } from '$lib/api/client';

import { EMOJI, LONG_KO, NO_SPACE } from './text';

export function comment(overrides: Partial<CommentView> = {}): CommentView {
  return {
    id: 'c-0001',
    parent_id: null,
    body: '초안 잘 봤습니다. 2절의 표는 다음 회의 전에 고쳐 두겠습니다.',
    author_id: 'u-0001',
    author_name: '김연구',
    created_at: '2026-10-20T01:30:00Z',
    edited: false,
    deleted: false,
    resolved: false,
    can_edit: true,
    can_delete: true,
    can_resolve: true,
    ...overrides,
  };
}

export const THREAD: CommentView[] = [
  comment(),
  comment({
    id: 'c-0002',
    parent_id: 'c-0001',
    body: '좋습니다. 표 번호도 같이 맞춰 주세요.',
    author_id: 'u-0002',
    author_name: '이협업',
    created_at: '2026-10-20T02:05:00Z',
    can_edit: false,
    can_delete: false,
  }),
  comment({
    id: 'c-0003',
    body: `${LONG_KO} ${NO_SPACE} ${EMOJI}`,
    author_id: 'u-0002',
    author_name: '이협업',
    created_at: '2026-10-21T09:00:00Z',
    edited: true,
    can_edit: false,
    can_delete: false,
    can_resolve: false,
  }),
  comment({
    id: 'c-0004',
    body: '참고 문헌 형식은 해결했습니다.',
    created_at: '2026-10-19T05:00:00Z',
    resolved: true,
  }),
];
