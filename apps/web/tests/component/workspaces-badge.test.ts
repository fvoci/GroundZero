import { describe, expect, it } from 'vitest';

import { badgeColor, workspaceHint, workspaceLabel } from '$lib/features/workspaces/badge';

const a = { id: '0198f1f2-0000-7000-8000-00000000a3f2', name: '연구실' };
const b = { id: '0198f1f2-0000-7000-8000-00000000b71c', name: '연구실' };
const c = { id: '0198f1f2-0000-7000-8000-00000000c001', name: '운영' };

describe('workspace badges (D-17, O-33)', () => {
  it('adds a short id only when names collide', () => {
    const all = [a, b, c];
    expect(workspaceLabel(a, all)).toBe('연구실 #a3f2');
    expect(workspaceLabel(b, all)).toBe('연구실 #b71c');
    expect(workspaceLabel(c, all)).toBe('운영');
    expect(workspaceHint(a, [a, c])).toBeNull();
  });

  it('picks a stable color from the id', () => {
    expect(badgeColor(a.id)).toBe(badgeColor(a.id));
    expect(badgeColor(a.id)).toMatch(/^bg-[a-z]+-500$/);
  });
});
