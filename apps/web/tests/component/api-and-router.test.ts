import { describe, expect, it } from 'vitest';

import { parseDocNode, parseDocument } from '$lib/api/client';
import { plainText } from '$lib/features/documents/drafts';
import { parseRoute } from '$lib/router.svelte';

const id = '0198f1f2-0000-7000-8000-000000000001';
const ws = '0198f1f2-0000-7000-8000-000000000002';

describe('parseRoute', () => {
  it('maps paths to screens', () => {
    expect(parseRoute('/')).toEqual({ name: 'home' });
    expect(parseRoute(`/w/${ws}`)).toEqual({ name: 'workspace', workspaceId: ws });
    expect(parseRoute(`/w/${ws}/wiki/${id}`)).toEqual({
      name: 'document',
      workspaceId: ws,
      documentId: id,
    });
    expect(parseRoute('/w/not-a-uuid')).toEqual({ name: 'not-found' });
    expect(parseRoute('/notes')).toEqual({ name: 'notes' });
    expect(parseRoute('/shared')).toEqual({ name: 'shared' });
    expect(parseRoute(`/d/${id}`)).toEqual({ name: 'standalone-document', documentId: id });
    expect(parseRoute(`/w/${ws}/members`)).toEqual({ name: 'members', workspaceId: ws });
  });
});

describe('response validation', () => {
  const valid = {
    id,
    workspace_id: ws,
    reference: 'WIKI-1',
    title: '회의록',
    body: { type: 'doc', content: [] },
    schema_version: 1,
    revision: 2,
    updated_at: '2026-10-17T00:00:00Z',
    placement: 'wiki',
    access: 'write',
    can_edit: true,
    visibility: 'workspace',
    allow_copy: false,
    can_manage: false,
    can_delete: false,
  };

  it('accepts a well-formed document', () => {
    expect(parseDocument(valid).revision).toBe(2);
  });

  it('rejects a body that is not a doc node', () => {
    expect(() => parseDocument({ ...valid, body: { type: 'paragraph' } })).toThrow();
    expect(() => parseDocNode([])).toThrow();
  });

  it('accepts a personal note without workspace or reference', () => {
    const note = parseDocument({
      ...valid,
      workspace_id: null,
      reference: null,
      placement: 'personal',
      access: 'owner',
      visibility: 'private',
    });
    expect(note.workspace_id).toBeNull();
    expect(note.reference).toBeNull();
  });

  it('rejects unknown access or placement', () => {
    expect(() => parseDocument({ ...valid, access: 'admin' })).toThrow();
    expect(() => parseDocument({ ...valid, placement: 'other' })).toThrow();
  });

  it('rejects missing or mistyped fields', () => {
    expect(() => parseDocument({ ...valid, revision: '2' })).toThrow();
    expect(() => parseDocument({ ...valid, can_edit: undefined })).toThrow();
  });
});

describe('plainText', () => {
  it('extracts readable text for conflict comparison', () => {
    const body = {
      type: 'doc',
      content: [
        { type: 'paragraph', content: [{ type: 'text', text: '첫 줄' }] },
        { type: 'paragraph', content: [{ type: 'text', text: '둘째 줄' }] },
      ],
    };
    expect(plainText(body)).toBe('첫 줄\n둘째 줄');
  });
});
