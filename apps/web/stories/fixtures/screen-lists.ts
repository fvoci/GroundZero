import type { SummaryView, TrashEntryView, TrashView } from '$lib/api/client';
import type { AuditEntryView } from '$lib/features/activity/api';

import { DOCS, personName } from './synthetic';
import { EMOJI, LONG_KO, NO_SPACE } from './text';

/** 현재 사용자 본인의 개인 노트만. seed 문서의 제목·트리를 개인 목록 계약으로 옮긴다. */
export const MY_NOTES: SummaryView[] = DOCS.map((doc) => ({
  ...doc,
  id: `note-${doc.id}`,
  reference: null,
  parent_id: doc.parent_id === null ? null : `note-${doc.parent_id}`,
}));

/** seed와 같은 합성 문구. 삭제 묶음의 종류·만료·참조 보존을 한 화면에서 확인한다. */
export const TRASH: TrashView = {
  retention_days: 30,
  version_retention_days: null,
  entries: (['document', 'task', 'item', 'module', 'event', 'file'] as const).map(
    (kind, index): TrashEntryView => ({
      id: `trash-${kind}`,
      root_id: `deleted-${kind}`,
      kind,
      title: [LONG_KO, NO_SPACE, EMOJI][index % 3] ?? LONG_KO,
      member_count: kind === 'module' ? 8 : kind === 'item' ? 3 : 1,
      deleted_by: 'story-user',
      deleted_by_name: personName('owner'),
      deleted_at: index < 2 ? '2026-10-19T01:30:00Z' : '2026-09-01T01:30:00Z',
      expires_at: index < 2 ? '2026-11-18T01:30:00Z' : '2026-10-01T01:30:00Z',
      expiry: index < 2 ? 'active' : index === 2 ? 'kept_for_references' : 'expired',
      can_purge: true,
    }),
  ),
};

/** 공간 감사 행만 담는다. 개인 노트·시간 기록의 내용은 공간 관리자에게도 넣지 않는다. */
export const AUDIT: AuditEntryView[] = TRASH.entries.map((entry, index) => ({
  id: `audit-${entry.kind}`,
  action: index < 2 ? 'delete' : index === 2 ? 'restore' : 'purge',
  target_type: entry.kind,
  target_id: entry.root_id,
  actor: index === 3 ? null : entry.deleted_by,
  actor_name: index === 3 ? null : (entry.deleted_by_name ?? null),
  detail: { title: entry.title },
  created_at: `2026-10-20T01:${String(35 - index).padStart(2, '0')}:00Z`,
}));
