import { keepAllDrafts, stopKeepingDrafts } from '$lib/features/common/leave-guard.svelte';
import { watchPreloadErrors, type PreloadReloadOptions } from '$lib/features/common/preload-reload';

import { reportNewVersion } from './feedback.svelte';

/**
 * 앱 본체의 새 버전 배포 처리. 옛 청크를 못 읽으면(`vite:preloadError`) 붙어 있는 이탈 가드·초안의 남기기로 미저장 입력을
 * 모두 초안으로 남긴 뒤 다시 읽는다(D-27·D-96과 같은 경로). 하나라도 남기지 못했거나, 다시 읽어 뜬 직후(같은 실패 묶음,
 * `preload-reload.ts`) 또 실패하면 다시 읽지 않고 저장한 뒤 새로고침하라는 배너를 띄운다(D-82). 앱 시작 때 한 번. 정리
 * 함수를 돌려준다.
 */
export function watchNewVersion(
  options: Pick<PreloadReloadOptions, 'reload' | 'storage'> = {},
): () => void {
  return watchPreloadErrors({
    ...options,
    ready: keepAllDrafts,
    // 다시 읽기가 일어나지 않았으면 창 떠나기 경고를 보통 규칙으로 되돌린다(R5 B 최종 C-1).
    cancelReload: stopKeepingDrafts,
    onStale: reportNewVersion,
  });
}
