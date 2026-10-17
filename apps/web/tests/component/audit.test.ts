import { describe, expect, it } from 'vitest';

import { auditSubject } from '$lib/features/audit/labels';
import { auditPath, parseRoute } from '$lib/router.svelte';

const W = '11111111-1111-1111-1111-111111111111';

describe('audit log', () => {
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
});
