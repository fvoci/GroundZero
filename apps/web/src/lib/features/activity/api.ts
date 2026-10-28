import type { Activity, ActivityPage, components } from '@fvoci/api-client';
import { expectOk } from '$lib/shared/api';
import {
  bool,
  int,
  isRecord,
  optStr,
  parseList,
  rec,
  str,
  none,
  type Json,
} from '$lib/shared/api/parse';
export type MarkTarget = components['schemas']['MarkTarget'];

// --- 활동 (D-119, 감사와 분리) ---------------------------------------------

export function parseActivity(value: Json): Activity {
  const o = rec(value, 'activity');
  const detail: unknown = o.detail;
  return {
    id: str(o, 'id'),
    kind: str(o, 'kind'),
    actor_id: optStr(o, 'actor_id'),
    actor_name: optStr(o, 'actor_name'),
    workspace_id: optStr(o, 'workspace_id'),
    target_kind: str(o, 'target_kind'),
    target_id: str(o, 'target_id'),
    item_id: optStr(o, 'item_id'),
    detail: isRecord(detail) ? detail : {},
    created_at: str(o, 'created_at'),
  };
}

export function parseActivityPage(value: Json): ActivityPage {
  const o = rec(value, 'activity page');
  return {
    activity: parseList(o.activity, parseActivity),
    next_before: optStr(o, 'next_before'),
    next_before_id: optStr(o, 'next_before_id'),
  };
}

/** 커서 조회(D-118·D-119): `before`와 `before_id`는 함께 보낸다. */
export interface PageCursor {
  before: string;
  before_id: string;
}

function cursorParams(
  params: URLSearchParams,
  cursor: PageCursor | undefined,
  limit?: number,
): void {
  if (cursor !== undefined) {
    params.set('before', cursor.before);
    params.set('before_id', cursor.before_id);
  }
  if (limit !== undefined) params.set('limit', String(limit));
}

export type NotificationView = Omit<
  Required<components['schemas']['Notification']>,
  'detail' | 'read_at'
>;

export interface NotificationPageView {
  notifications: NotificationView[];
  /** 둘 다 null이면 더 읽을 페이지가 없다(D-118). */
  next: PageCursor | null;
}

export function parseNotificationPage(value: Json): NotificationPageView {
  const o = rec(value, 'notification page');
  const before = optStr(o, 'next_before');
  const beforeId = optStr(o, 'next_before_id');
  return {
    notifications: parseList(o.notifications, parseNotification),
    next: before !== null && beforeId !== null ? { before, before_id: beforeId } : null,
  };
}

export function parseNotification(value: Json): NotificationView {
  const o = rec(value, 'notification');
  return {
    id: str(o, 'id'),
    kind: str(o, 'kind'),
    organization_id: optStr(o, 'organization_id'),
    invitation_id: optStr(o, 'invitation_id'),
    document_id: optStr(o, 'document_id'),
    task_id: optStr(o, 'task_id'),
    // 초대 알림은 대상 제목이 없다(D-118).
    document_title: optStr(o, 'document_title'),
    document_reference: optStr(o, 'document_reference'),
    workspace_id: optStr(o, 'workspace_id'),
    item_id: optStr(o, 'item_id'),
    comment_id: optStr(o, 'comment_id'),
    excerpt: optStr(o, 'excerpt'),
    due_date: optStr(o, 'due_date'),
    subject: optStr(o, 'subject'),
    actor_name: optStr(o, 'actor_name'),
    created_at: str(o, 'created_at'),
    read: bool(o, 'read'),
  };
}

export type MarkKind = components['schemas']['MarkKind'];

function parseMarkState(value: Json): { pin: boolean; favorite: boolean } {
  const o = rec(value, 'mark state');
  return { pin: bool(o, 'pin'), favorite: bool(o, 'favorite') };
}

export type AuditEntryView = Required<components['schemas']['AuditEntry']>;

function parseAudit(value: Json): AuditEntryView {
  const o = rec(value, 'audit entry');
  return {
    id: str(o, 'id'),
    action: str(o, 'action'),
    target_type: str(o, 'target_type'),
    target_id: str(o, 'target_id'),
    actor: optStr(o, 'actor'),
    actor_name: optStr(o, 'actor_name'),
    detail: o.detail,
    created_at: str(o, 'created_at'),
  };
}

export type CommentView = Required<components['schemas']['Comment']>;

export interface CommentTarget {
  kind: 'document' | 'task';
  id: string;
}

function parseComment(value: Json): CommentView {
  const o = rec(value, 'comment');
  return {
    id: str(o, 'id'),
    parent_id: optStr(o, 'parent_id'),
    body: str(o, 'body'),
    author_id: str(o, 'author_id'),
    author_name: optStr(o, 'author_name'),
    created_at: str(o, 'created_at'),
    edited: bool(o, 'edited'),
    deleted: bool(o, 'deleted'),
    resolved: bool(o, 'resolved'),
    can_edit: bool(o, 'can_edit'),
    can_delete: bool(o, 'can_delete'),
    can_resolve: bool(o, 'can_resolve'),
  };
}
export const activityApi = {
  comments: (target: CommentTarget) =>
    expectOk(
      'GET',
      `/api/${target.kind === 'document' ? 'documents' : 'tasks'}/${encodeURIComponent(target.id)}/comments`,
      (v) => parseList(v, parseComment),
    ),

  addComment: (target: CommentTarget, body: string, parentId: string | null, requestId: string) =>
    expectOk('POST', '/api/comments', (v) => str(rec(v, 'comment created'), 'id'), {
      request_id: requestId,
      document_id: target.kind === 'document' ? target.id : null,
      task_id: target.kind === 'task' ? target.id : null,
      parent_id: parentId,
      body,
    }),

  editComment: (id: string, body: string) =>
    expectOk('PUT', `/api/comments/${encodeURIComponent(id)}`, none, { body }),

  deleteComment: (id: string) =>
    expectOk('DELETE', `/api/comments/${encodeURIComponent(id)}`, none),

  resolveComment: (id: string, resolved: boolean) =>
    expectOk('POST', `/api/comments/${encodeURIComponent(id)}/resolve`, none, { resolved }),

  /** 감사 기록(워크스페이스, 또는 null이면 내 개인 영역). before는 마지막으로 받은 기록 id. */
  audit: (workspaceId: string | null, before?: string) => {
    const base =
      workspaceId === null
        ? '/api/me/audit'
        : `/api/workspaces/${encodeURIComponent(workspaceId)}/audit`;
    const query = before === undefined ? '' : `?before=${encodeURIComponent(before)}`;
    return expectOk('GET', `${base}${query}`, (v) => parseList(v, parseAudit));
  },

  /** 현재 조직의 최근 활동(D-119). */
  recentActivity: (organizationId: string, limit?: number) => {
    const params = new URLSearchParams({ organization_id: organizationId });
    if (limit !== undefined) params.set('limit', String(limit));
    return expectOk('GET', `/api/me/activity?${params.toString()}`, (v) =>
      parseList(v, parseActivity),
    );
  },

  /** 항목·문서·업무의 활동(대상을 볼 수 있으면, D-119). */
  targetActivity: (
    kind: 'item' | 'document' | 'task',
    id: string,
    cursor?: PageCursor,
    limit?: number,
  ) => {
    const params = new URLSearchParams();
    cursorParams(params, cursor, limit);
    const base = `/api/${kind === 'item' ? 'items' : kind === 'document' ? 'documents' : 'tasks'}/${encodeURIComponent(id)}/activity`;
    const query = params.toString();
    return expectOk('GET', query === '' ? base : `${base}?${query}`, parseActivityPage);
  },

  mark: (kind: MarkTarget, id: string) =>
    expectOk('GET', `/api/me/marks/${kind}/${encodeURIComponent(id)}`, parseMarkState),

  setMark: (kind: MarkTarget, id: string, mark: MarkKind, on: boolean) =>
    expectOk('PUT', `/api/me/marks/${kind}/${encodeURIComponent(id)}`, parseMarkState, {
      mark,
      on,
    }),

  /** 알림함은 현재 조직 범위이며 초대는 어느 조직 문맥에서나 보인다. 목록은 커서로 더 본다(D-118). */
  notifications: (organizationId: string, cursor?: PageCursor, limit?: number) => {
    const params = new URLSearchParams({ organization_id: organizationId });
    cursorParams(params, cursor, limit);
    return expectOk('GET', `/api/me/notifications?${params.toString()}`, parseNotificationPage);
  },

  /** 안 읽은 수는 서버가 센다(D-118). */
  unreadCount: (organizationId: string) =>
    expectOk(
      'GET',
      `/api/me/notifications/unread-count?organization_id=${encodeURIComponent(organizationId)}`,
      (v) => int(rec(v, 'unread count'), 'unread_count'),
    ),

  /** id가 null이면 현재 조직의 모두 읽음(초대 포함). 읽음 처리한 수를 돌려준다. */
  markNotificationsRead: (organizationId: string, id: string | null) =>
    expectOk(
      'POST',
      `/api/me/notifications/read?organization_id=${encodeURIComponent(organizationId)}`,
      (v) => int(rec(v, 'marked notifications'), 'marked'),
      { id },
    ),
};
