// 현재 조직 문맥(D-111·D-121). 개인 화면(홈·내 업무·알림함·공유받은 문서·검색)은 현재 조직 범위이고 서버는
// 요청마다 `organization_id`를 받는다(없으면 400, 멤버가 아니면 404). 서버는 기본 조직을 정하지 않으므로 선택은 화면의
// 몫이다. 경로의 조직, 서버에 기억한 선택, 유일한 조직 순서로 정한다. 여러 조직 가운데 첫 조직을 조용히 고르지 않는다.
// 마지막 선택은 서버에 저장해 기기 간 이어지고 브라우저 사본은 사용자별 키를 유지한다(사용자 ①·⑩, 2026-10-26).
import type { OrganizationSummary } from '@fvoci/api-client';

import { api } from '$lib/api/client';
import { errorText } from '$lib/api/errors';
import { accountSwitching, notifyError } from '$lib/features/feedback/feedback.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { readLocal, scopedKey, writeLocal } from '$lib/storage';

const SELECTED = 'organization';

class OrganizationContext {
  /** 내가 속한 조직(`GET /api/me/organizations`). 로그인 뒤 채우고 초대 수락·탈퇴 뒤 다시 읽는다. */
  organizations = $state<OrganizationSummary[]>([]);
  /** 사용자가 고른 조직. 목록에 없으면 무시한다. */
  private chosenId = $state<string | undefined>(undefined);
  /** 지금 보는 워크스페이스의 조직(화면이 경로에서 정해 준다). 있으면 선택보다 우선한다. */
  routeOrganizationId = $state<string | undefined>(undefined);
  private userId = '';
  private generation = 0;
  private saves: Promise<void> = Promise.resolve();

  /** 현재 조직. 경로의 조직 › 기억한 선택 › 유일한 조직 › 없음. */
  readonly currentId: string | undefined = $derived.by(() => {
    const ids = new Set(this.organizations.map((o) => o.id));
    if (this.routeOrganizationId !== undefined && ids.has(this.routeOrganizationId))
      return this.routeOrganizationId;
    if (this.chosenId !== undefined && ids.has(this.chosenId)) return this.chosenId;
    if (this.organizations.length === 1) return this.organizations[0]?.id;
    return undefined;
  });

  readonly current: OrganizationSummary | undefined = $derived(
    this.organizations.find((o) => o.id === this.currentId),
  );

  /**
   * `chosen`은 서버의 마지막 선택(null은 명시적으로 없음). 생략하면 멤버십 다시 읽기이므로 현재 선택을 유지한다.
   * 로그인은 항상 서버 값을 넘겨 브라우저의 옛 선택이 서버 설정을 되살리지 않게 한다.
   */
  load(userId: string, organizations: OrganizationSummary[], chosen?: string | null): void {
    // 같은 사용자의 다시 읽기(초대 수락·탈퇴 뒤)는 지금 선택을 잊지 않는다. 다른 사용자면 그 사용자의 기억만 쓴다.
    const kept = this.userId === userId ? this.chosenId : undefined;
    if (this.userId !== userId) {
      this.generation += 1;
      this.saves = Promise.resolve();
    }
    this.userId = userId;
    this.organizations = organizations;
    const selected =
      chosen === undefined
        ? (kept ?? readLocal(scopedKey(SELECTED, userId, undefined)) ?? undefined)
        : (chosen ?? undefined);
    this.chosenId = organizations.some((o) => o.id === selected) ? selected : undefined;
    if (chosen !== undefined) {
      writeLocal(scopedKey(SELECTED, userId, undefined), this.chosenId ?? null);
    }
  }

  /** 전환기에서 고른 조직. 목록에 있는 조직만 받는다. */
  select(organizationId: string): void {
    if (!this.organizations.some((o) => o.id === organizationId)) return;
    if (this.chosenId === organizationId) return;
    this.chosenId = organizationId;
    const userId = this.userId;
    const generation = this.generation;
    if (userId === '') return;
    writeLocal(scopedKey(SELECTED, userId, undefined), organizationId);
    // 같은 계정의 연속 선택은 요청 순서를 지킨다. 계정 전환 뒤 아직 보내지 않은 이전 선택은 폐기한다.
    this.saves = this.saves.then(async () => {
      if (generation !== this.generation || me.userId !== userId || accountSwitching()) return;
      try {
        await api.setDisplaySettings({ last_organization_id: organizationId });
      } catch (error: unknown) {
        if (generation === this.generation && this.chosenId === organizationId) {
          notifyError(errorText(error, m.settings_save_failed()), 'organization-selection');
        }
      }
    });
  }

  /** 로그아웃·계정 전환 때 비운다. 기억은 사용자별 키라 지우지 않는다. */
  reset(): void {
    this.generation += 1;
    this.saves = Promise.resolve();
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
