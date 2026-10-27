import { keepAllDrafts } from '$lib/features/common/leave-guard.svelte';
import { watchPreloadErrors, type PreloadReloadOptions } from '$lib/features/common/preload-reload';

import { reportNewVersion } from './feedback.svelte';

/**
 * 앱 본체의 새 버전 배포 처리. 옛 청크를 못 읽으면(`vite:preloadError`) 붙어 있는 이탈 가드·초안의 남기기로 미저장 입력을
 * 모두 초안으로 남긴 뒤 한 번만 다시 읽는다(D-27·D-96과 같은 경로). 하나라도 남기지 못했거나 이미 한 번 다시 읽었으면
 * 다시 읽지 않고 저장한 뒤 새로고침하라는 배너를 띄운다(D-82). 앱 시작 때 한 번. 정리 함수를 돌려준다.
 */
export function watchNewVersion(
  options: Pick<PreloadReloadOptions, 'reload' | 'storage'> = {},
): () => void {
  return watchPreloadErrors({ ...options, ready: keepAllDrafts, onStale: reportNewVersion });
}
