import type { Workspace } from '@fvoci/api-client';
import { render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { AuditEntryView } from '$lib/api/client';
import { api } from '$lib/api/client';
import AuditScreen from '$lib/features/audit/AuditScreen.svelte';
import { auditAction, auditSubject } from '$lib/features/audit/labels';
import { setLocale } from '$lib/paraglide/runtime.js';

// 감사 기록은 번역된 글로 보인다(D-81, 감사 W32). 서버가 남기는 보관·템플릿·실시간 동작에 이름이 있고, 역할·
// 공개 범위·상태 코드는 표시 이름으로, 사용자 id(리더)는 그대로 보이지 않는다.
const LEADER = '0198f1f2-0000-7000-8000-0000000000aa';

describe('audit codes', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('names the archive, template and realtime actions in both languages', async () => {
    expect(auditAction('archive')).toBe('보관');
    expect(auditAction('unarchive')).toBe('보관 해제');
    expect(auditAction('mark_template')).toBe('템플릿으로 표시');
    expect(auditAction('unmark_template')).toBe('템플릿 해제');
    expect(auditAction('set_realtime')).toBe('실시간 공동 편집 설정 바꿈');
    await setLocale('en', { reload: false });
    expect(auditAction('archive')).toBe('archived');
    expect(auditAction('set_realtime')).toBe('real-time co-editing changed');
  });

  it('shows role, visibility and status codes by name and hides user ids', () => {
    expect(auditSubject({ from: 'readonly', to: 'member' }, 'change_role')).toBe('멤버');
    expect(auditSubject({ from: 'workspace', to: 'private' }, 'change_visibility')).toBe('비공개');
    expect(auditSubject({ from: 'active', to: 'on_hold' }, 'change_status')).toBe('보류');
    expect(auditSubject({ from: null, to: LEADER }, 'change_leader')).toBe('');
    expect(auditSubject({ enabled: true }, 'set_realtime')).toBe('켬');
    expect(auditSubject({ enabled: false }, 'set_realtime')).toBe('끔');
    // 모르는 코드는 원문 대신 비운다.
    expect(auditSubject({ from: 'readonly', to: 'owner' }, 'change_role')).toBe('');
    // 이름 바꾸기의 `to`는 새 제목이라 그대로 보인다.
    expect(auditSubject({ from: '초안', to: '회의록' }, 'rename')).toBe('회의록');
  });

  it('renders the audit list without raw codes or user ids', async () => {
    const workspace: Workspace = {
      access: 'owner',
      id: 'ws-1',
      organization_id: 'org-1',
      name: '연구실',
      role: 'admin',
      is_owner: true,
      realtime_enabled: false,
    };
    const entry = (
      id: string,
      action: string,
      target: string,
      detail: unknown,
    ): AuditEntryView => ({
      id,
      action,
      target_type: target,
      target_id: 't',
      actor: 'u1',
      actor_name: '관리자',
      detail,
      created_at: '2026-10-21T00:00:00Z',
    });
    vi.spyOn(api, 'audit').mockResolvedValue([
      entry('a1', 'archive', 'module', {}),
      entry('a2', 'mark_template', 'document', {}),
      entry('a3', 'set_realtime', 'workspace', { enabled: true }),
      entry('a4', 'change_role', 'member', { from: 'readonly', to: 'member' }),
      entry('a5', 'change_status', 'item', { from: 'active', to: 'on_hold' }),
      entry('a6', 'change_leader', 'item', { from: null, to: LEADER }),
    ]);
    render(AuditScreen, { props: { workspace } });
    const list = await screen.findByTestId('audit-list');
    await vi.waitFor(() => {
      expect(list.querySelectorAll('li')).toHaveLength(6);
    });
    const text = list.textContent;
    for (const raw of ['archive', 'mark_template', 'set_realtime', 'member', 'on_hold', LEADER]) {
      expect(text).not.toContain(raw);
    }
    expect(text).toContain('모듈 보관');
    expect(text).toContain('멤버');
    expect(text).toContain('보류');
  });
});
