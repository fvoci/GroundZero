import { beforeEach, describe, expect, it } from 'vitest';

import { auditAction, auditSubject, auditTarget } from '$lib/features/audit/labels';
import { setLocale } from '$lib/paraglide/runtime.js';
import { auditPath, parseRoute } from '$lib/router.svelte';

const W = '11111111-1111-1111-1111-111111111111';

describe('audit log', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });

  it('has a workspace and a personal route', () => {
    expect(parseRoute(auditPath(W))).toEqual({ name: 'audit', workspaceId: W });
    expect(parseRoute('/audit')).toEqual({ name: 'my-audit' });
  });

  it('shows a readable subject from the detail', () => {
    expect(auditSubject({ title: '회의록', batch: 'x' })).toBe('회의록');
    expect(auditSubject({ email: 'a@example.test' })).toBe('a@example.test');
    expect(auditSubject(null)).toBe('');
    expect(auditSubject({ count: 3 })).toBe('');
  });

  it('names known actions and targets per language and keeps unknown codes', async () => {
    expect(auditTarget('document')).toBe('문서');
    expect(auditAction('create')).toBe('만듦');
    expect(auditAction('toString')).toBe('toString');
    expect(auditTarget('new_kind')).toBe('new_kind');
    await setLocale('en', { reload: false });
    expect(auditAction('create')).toBe('created');
  });
});
