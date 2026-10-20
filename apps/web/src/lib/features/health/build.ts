// 실행 중인 빌드의 표시(D-87). 값은 서버 `/healthz`의 `build`를 그대로 쓴다(화면이 따로 버전을 들지 않는다).
import type { Health } from '@fvoci/api-client';
import { m } from '$lib/paraglide/messages.js';

/** 메뉴에 보일 한 줄. 개발 빌드는 릴리스로 오인되지 않게 따로 표시한다. */
export function buildLabel(build: Health['build'] | undefined): string {
  if (build === undefined) return m.app_build_unknown();
  if (build.version === 'dev') return m.app_build_dev();
  return m.app_build_release({ version: build.version, source: build.source.slice(0, 7) });
}
