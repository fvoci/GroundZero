import type { components } from '@fvoci/api-client';
import {
  parseDocument,
  parseMoveOutcome,
  parseMovePreview,
  type DocumentView,
  type MoveOutcomeView,
  type MovePreviewView,
  type MoveTargetInput,
} from '$lib/api/client';
import { EXPECTED_USER, expectOk, fail, request } from '$lib/shared/api';
import { isRecord, rec, str, strings, type Json } from '$lib/shared/api/parse';

type StaleWrite = components['schemas']['StaleWrite_Document'];
export type DocumentConflict = {
  kind: 'conflict';
  current: DocumentView;
  changed: StaleWrite['changed'];
  requestId: StaleWrite['request_id'];
};

// Only revision conflicts carry a resource. Room failures and account changes remain ErrorBody.
function staleWrite(status: number, json: Json): DocumentConflict | undefined {
  if (status !== 409 || !isRecord(json) || json.error !== 'conflict' || json.current === undefined)
    return undefined;
  const body = rec(json, 'document conflict');
  return {
    kind: 'conflict',
    current: parseDocument(body.current),
    changed: strings(body.changed),
    requestId: str(body, 'request_id'),
  };
}

export const documentsApi = {
  setSettings: (
    id: string,
    change: { visibility?: 'workspace' | 'private'; allow_copy?: boolean },
  ) =>
    expectOk('PATCH', `/api/documents/${encodeURIComponent(id)}/settings`, parseDocument, change),

  movePreview: (id: string, target: MoveTargetInput) =>
    expectOk('POST', `/api/documents/${encodeURIComponent(id)}/move-plan`, parseMovePreview, {
      target_workspace_id: target.workspaceId,
      target_item_id: target.itemId,
    }),

  async moveDocument(
    id: string,
    target: MoveTargetInput,
    acknowledged: MovePreviewView,
  ): Promise<
    { kind: 'moved'; outcome: MoveOutcomeView } | { kind: 'changed'; preview: MovePreviewView }
  > {
    const { status, json } = await request(
      'PUT',
      `/api/documents/${encodeURIComponent(id)}/placement`,
      {
        target_workspace_id: target.workspaceId,
        target_item_id: target.itemId,
        acknowledged: { changes: acknowledged.changes },
      },
    );
    if (status === 409 && isRecord(json) && 'changes' in json)
      return { kind: 'changed', preview: parseMovePreview(json) };
    if (status !== 200) fail(status, json);
    return { kind: 'moved', outcome: parseMoveOutcome(json) };
  },

  async patchTitle(
    id: string,
    input: { expectedUser: string; requestId: string; baseRevision: number; title: string },
  ): Promise<{ kind: 'saved'; document: DocumentView } | DocumentConflict> {
    const { status, json } = await request(
      'PATCH',
      `/api/documents/${encodeURIComponent(id)}`,
      {
        request_id: input.requestId,
        base_revision: input.baseRevision,
        title: input.title,
      },
      { [EXPECTED_USER]: input.expectedUser },
    );
    const conflict = staleWrite(status, json);
    if (conflict !== undefined) return conflict;
    if (status !== 200) fail(status, json);
    return { kind: 'saved', document: parseDocument(json) };
  },

  async restoreRevision(
    id: string,
    revision: number,
    input: { requestId: string; baseRevision: number },
  ): Promise<{ kind: 'restored'; document: DocumentView } | DocumentConflict> {
    const { status, json, etag } = await request(
      'POST',
      `/api/documents/${encodeURIComponent(id)}/revisions/${String(revision)}/restore`,
      { request_id: input.requestId, base_revision: input.baseRevision },
    );
    const conflict = staleWrite(status, json);
    if (conflict !== undefined) return conflict;
    if (status !== 200) fail(status, json);
    const document = parseDocument(json);
    return {
      kind: 'restored',
      document: etag === null || etag === '' ? document : { ...document, etag },
    };
  },
};
