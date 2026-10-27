import type { Activity } from '@fvoci/api-client';
import { beforeEach, expect, it } from 'vitest';

import { activityText } from '$lib/features/activity/activity';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

// R5 F-12: 활동 문구는 서버 `activity_kind_check`(create·move·comment·status·kanban_move·assign·unassign·unshare)와
// `activity_target_kind_check`(workspace·module·item·document·task·event·file·record)를 따른다.
const base: Activity = {
  id: 'a1',
  actor_id: 'u1',
  actor_name: '가람',
  created_at: '2026-10-27T00:00:00Z',
  detail: {},
  item_id: null,
  kind: 'create',
  target_id: '00000000-0000-4000-8000-000000000001',
  target_kind: 'task',
  workspace_id: null,
};

beforeEach(async () => {
  await setLocale('ko', { reload: false });
});

it('describes a revoked share with its own sentence', () => {
  expect(activityText({ ...base, kind: 'unshare', target_kind: 'document' })).toBe(
    m.home_activity_unshare({ who: '가람', target: m.home_activity_target_document() }),
  );
});

it('names every server target kind when the activity has no title', () => {
  const labels: Record<string, string> = {
    workspace: m.home_activity_target_workspace(),
    module: m.home_activity_target_module(),
    item: m.home_activity_target_item(),
    document: m.home_activity_target_document(),
    task: m.home_activity_target_task(),
    event: m.home_activity_target_event(),
    file: m.home_activity_target_file(),
    record: m.home_activity_target_record(),
  };
  for (const [kind, target] of Object.entries(labels)) {
    expect(activityText({ ...base, target_kind: kind })).toBe(
      m.home_activity_create({ who: '가람', target }),
    );
  }
  expect(new Set(Object.values(labels)).size).toBe(8);
});

it('uses the title when there is one and the general sentence for unknown kinds', () => {
  expect(activityText({ ...base, kind: 'detach', detail: { title: '보고서' } })).toBe(
    m.home_activity_other({ who: '가람', target: '보고서' }),
  );
});

// 서버는 담당 해제 활동에 까닭을 남긴다(`detail.reason`, SQL app_drop_unseen_assignees). 멤버를 내보내 풀린 담당
// (`app_remove_member_internal`의 'remove')만 그 문장이고, 다른 까닭(역할 변경·그룹 삭제 등)이나 까닭이 없으면 보통의
// 담당 해제다(R5 B 최종 FC-20).
it('tells a member removal apart from other unassignments', () => {
  const unassign = { ...base, kind: 'unassign', detail: { title: '보고서' } };
  expect(activityText({ ...unassign, detail: { title: '보고서', reason: 'remove' } })).toBe(
    m.home_activity_member_removed({ who: '가람', target: '보고서' }),
  );
  for (const reason of ['role_change', 'group_deleted', 'organization_left'])
    expect(activityText({ ...unassign, detail: { title: '보고서', reason } })).toBe(
      m.home_activity_unassign({ who: '가람', target: '보고서' }),
    );
  expect(activityText(unassign)).toBe(m.home_activity_unassign({ who: '가람', target: '보고서' }));
});
