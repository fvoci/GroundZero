import type { OrganizationSummary } from '@fvoci/api-client';

/** 컴포넌트 시험의 기본 조직(D-111). `tests/setup.ts`가 모든 시험을 이 조직을 고른 상태로 시작한다. */
export const TEST_ORGANIZATION: OrganizationSummary = {
  id: 'org-1',
  name: '시험 조직',
  role: 'admin',
  public_links_enabled: true,
  created_at: '2026-10-01T00:00:00Z',
};
