// 현재 조직 문맥(D-111·D-121). 개인 화면(홈·내 업무·알림함·공유받은 문서·검색)은 현재 조직 범위이고 서버는
// 요청마다 `organization_id`를 받는다(없으면 400, 멤버가 아니면 404). 서버는 기본 조직을 정하지 않으므로 선택은 화면의
// 몫이다. 우선순위: (1) 지금 보는 워크스페이스의 조직 (2) 이 브라우저에 기억한 선택(사용자별 키) (3) 없음 → 개인 화면이
// 고르게 한다. 여러 조직 가운데 첫 조직을 조용히 고르지 않는다(사용자 결정 2026-10-26). 조직이 하나뿐일 때의 자동 선택
// 여부는 결정 대기이며, 그때까지는 그 하나도 사용자가 고른다.
import type { OrganizationSummary } from '@fvoci/api-client';

import { readLocal, scopedKey, writeLocal } from '$lib/storage';

const SELECTED = 'organization';

class OrganizationContext {
  /** 내가 속한 조직(`GET /api/me/organizations`). 로그인 뒤 채우고 초대 수락·탈퇴 뒤 다시 읽는다. */
  organizations = $state<OrganizationSummary[]>([]);
  /** 사용자가 고른 조직(이 브라우저의 기억). 목록에 없으면 무시한다. */
  private chosenId = $state<string | undefined>(undefined);
  /** 지금 보는 워크스페이스의 조직(화면이 경로에서 정해 준다). 있으면 선택보다 우선한다. */
  routeOrganizationId = $state<string | undefined>(undefined);
  private userId = '';

  /** 현재 조직. 경로의 조직 › 기억한 선택 › 없음. */
  readonly currentId: string | undefined = $derived.by(() => {
    const ids = new Set(this.organizations.map((o) => o.id));
    if (this.routeOrganizationId !== undefined && ids.has(this.routeOrganizationId))
      return this.routeOrganizationId;
    if (this.chosenId !== undefined && ids.has(this.chosenId)) return this.chosenId;
    return undefined;
  });

  readonly current: OrganizationSummary | undefined = $derived(
    this.organizations.find((o) => o.id === this.currentId),
  );

  /**
   * 로그인한 사용자 기준으로 목록과 기억을 맞춘다. 다른 계정의 기억을 물려받지 않는다(D-96). `chosen`은 저장소 대신 쓸
   * 선택(시험·story가 저장소를 건드리지 않고 조직을 고른 상태로 시작할 때).
   */
  load(userId: string, organizations: OrganizationSummary[], chosen?: string): void {
    // 같은 사용자의 다시 읽기(초대 수락·탈퇴 뒤)는 지금 선택을 잊지 않는다. 다른 사용자면 그 사용자의 기억만 쓴다.
    const kept = this.userId === userId ? this.chosenId : undefined;
    this.userId = userId;
    this.organizations = organizations;
    this.chosenId =
      chosen ?? readLocal(scopedKey(SELECTED, userId, undefined)) ?? kept ?? undefined;
  }

  /** 전환기에서 고른 조직. 목록에 있는 조직만 받는다. */
  select(organizationId: string): void {
    if (!this.organizations.some((o) => o.id === organizationId)) return;
    this.chosenId = organizationId;
    if (this.userId !== '') writeLocal(scopedKey(SELECTED, this.userId, undefined), organizationId);
  }

  /** 로그아웃·계정 전환 때 비운다. 기억은 사용자별 키라 지우지 않는다. */
  reset(): void {
    this.userId = '';
    this.organizations = [];
    this.chosenId = undefined;
    this.routeOrganizationId = undefined;
  }

  /** 현재 조직 범위의 브라우저 저장소 키(D-121). */
  key(name: string): string {
    return scopedKey(name, this.userId, this.currentId);
  }
}

export const org = new OrganizationContext();
