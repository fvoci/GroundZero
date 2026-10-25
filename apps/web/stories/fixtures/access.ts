import type { DocumentView, FileView } from '$lib/api/client';

/** 권한·공개 링크 story와 컴포넌트 시험용 합성 문서. */
export const ACCESS_DOCUMENT: DocumentView = {
  id: 'doc-access',
  workspace_id: 'ws-1',
  placement: 'wiki',
  item_id: null,
  parent_id: null,
  reference: 'WIKI-1',
  title: '공유 안내',
  body: { type: 'doc', content: [] },
  schema_version: 1,
  revision: 1,
  updated_at: '2026-10-26T00:00:00Z',
  access: 'owner',
  can_edit: true,
  visibility: 'private',
  allow_copy: false,
  can_manage: true,
  can_delete: true,
  is_template: false,
  realtime: false,
  can_export: true,
};

export const ACCESS_FILE: FileView = {
  id: 'file-access',
  name: '공유 안내.pdf',
  document_id: ACCESS_DOCUMENT.id,
  item_id: null,
  current_version: 1,
  size: 1024,
  content_type: 'application/pdf',
  updated_at: ACCESS_DOCUMENT.updated_at,
  can_write: true,
  can_download: true,
};
