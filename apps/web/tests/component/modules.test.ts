import { describe, expect, it } from 'vitest';

import { parseDocument, parseItem, parseModule } from '$lib/api/client';
import { normalizeKey } from '$lib/features/modules/kinds';
import { parseRoute, pathFor } from '$lib/router.svelte';

const ws = '0198f1f2-0000-7000-8000-000000000002';
const mod = '0198f1f2-0000-7000-8000-000000000003';
const item = '0198f1f2-0000-7000-8000-000000000004';
const doc = '0198f1f2-0000-7000-8000-000000000005';

describe('module routes', () => {
  it('maps module, item and item document paths', () => {
    expect(parseRoute(`/w/${ws}/m/${mod}`)).toEqual({
      name: 'module',
      workspaceId: ws,
      moduleId: mod,
    });
    expect(parseRoute(`/w/${ws}/i/${item}`)).toEqual({
      name: 'item',
      workspaceId: ws,
      itemId: item,
    });
    expect(parseRoute(`/w/${ws}/i/${item}/d/${doc}`)).toEqual({
      name: 'item-document',
      workspaceId: ws,
      itemId: item,
      documentId: doc,
    });
  });

  it('opens documents under their item when I am a member', () => {
    const member = (): boolean => true;
    expect(pathFor({ id: doc, workspace_id: ws, item_id: item }, member)).toBe(
      `/w/${ws}/i/${item}/d/${doc}`,
    );
    expect(pathFor({ id: doc, workspace_id: ws, item_id: null }, member)).toBe(
      `/w/${ws}/wiki/${doc}`,
    );
    expect(pathFor({ id: doc, workspace_id: ws, item_id: item }, () => false)).toBe(`/d/${doc}`);
  });
});

describe('module validation', () => {
  it('normalizes keys like the server', () => {
    expect(normalizeKey(' dev ')).toBe('DEV');
    for (const bad of ['', 'D', '1AB', 'DE-V', 'ABCDEFGHIJK', 'wiki', '가나']) {
      expect(normalizeKey(bad)).toBeUndefined();
    }
  });

  it('parses modules and items and rejects unknown kinds', () => {
    const module = {
      id: mod,
      workspace_id: ws,
      kind: 'project',
      name: '개발',
      key: 'DEV',
      previous_keys: ['ENG'],
      archived: false,
      icon: '',
      description: '제품 개발',
      access: 'owner',
    };
    expect(parseModule(module).previous_keys).toEqual(['ENG']);
    expect(parseModule(module).description).toBe('제품 개발');
    expect(() => parseModule({ ...module, kind: 'chores' })).toThrow();
    const parsed = parseItem({
      id: item,
      module_id: mod,
      workspace_id: ws,
      reference: 'DEV-1',
      title: '항목',
      description: '',
      access: 'owner',
      visibility: 'private',
      archived: false,
      updated_at: '2026-10-17T00:00:00Z',
      status: 'active',
      starts_on: null,
      ends_on: '2026-12-31',
      is_template: false,
      version: 1,
      realtime: false,
      can_export: true,
    });
    expect(parsed.reference).toBe('DEV-1');
    expect(parsed.ends_on).toBe('2026-12-31');
    expect(() => parseItem({ ...parsed, access: 'admin' })).toThrow();
    expect(() => parseItem({ ...parsed, status: 'late' })).toThrow();
  });

  it('accepts module documents with an item', () => {
    const parsed = parseDocument({
      id: doc,
      workspace_id: ws,
      item_id: item,
      placement: 'module',
      reference: 'DEV-2',
      title: '설계',
      body: { type: 'doc' },
      schema_version: 1,
      revision: 1,
      updated_at: '2026-10-17T00:00:00Z',
      access: 'write',
      can_edit: true,
      visibility: 'workspace',
      allow_copy: false,
      can_manage: false,
      can_delete: false,
      is_template: false,
      realtime: false,
      can_export: true,
    });
    expect(parsed.item_id).toBe(item);
    expect(parsed.placement).toBe('module');
  });
});
