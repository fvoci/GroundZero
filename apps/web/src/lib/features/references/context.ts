// 편집기에 넘기는 참조 문맥: 후보를 찾을 워크스페이스와 대상 경로 계산.
import type { ResolvedReferenceView } from '$lib/api/client';
import { itemPath, itemTaskPath, pathFor } from '$lib/router.svelte';

import type { DescribeWorkspace } from './reference-extension';

export interface ReferenceContext {
  /** 후보를 찾을 워크스페이스. 개인 노트는 null. */
  workspaceId: string | null;
  /** 후보 목록에서 워크스페이스를 구분해 보여 준다(같은 이름이면 보조 정보). */
  describeWorkspace: DescribeWorkspace;
  hrefFor: (target: ResolvedReferenceView) => string | null;
  navigate: (path: string) => void;
}

/** 대상 경로. 사람은 경로가 없고, 내가 멤버가 아닌 워크스페이스의 항목은 열 수 없다. */
export function referenceHref(
  target: ResolvedReferenceView,
  memberOf: (ws: string) => boolean,
): string | null {
  if (!target.visible) return null;
  if (target.kind === 'item') {
    return target.workspace_id !== null && memberOf(target.workspace_id)
      ? itemPath(target.workspace_id, target.id)
      : null;
  }
  if (target.kind === 'task') {
    return target.workspace_id !== null && target.item_id !== null && memberOf(target.workspace_id)
      ? itemTaskPath(target.workspace_id, target.item_id, target.id)
      : null;
  }
  if (target.kind === 'document') {
    return pathFor(
      { id: target.id, workspace_id: target.workspace_id, item_id: target.item_id },
      memberOf,
    );
  }
  return null;
}
