// 모든 story에 공통인 준비. 제품의 전역 스타일·서체·번역을 그대로 쓰고, 표시 설정과 네트워크를 결정적으로 고정한다.
import type { Preview } from '@storybook/svelte';

import { asLocale, type DisplayLocale } from '$lib/display';
import { applyDocFont } from '$lib/doc-font';
import { watchPreloadErrors } from '$lib/features/common/preload-reload';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { applyTheme } from '$lib/theme.svelte';

import '../src/styles/app.css';
import '../src/styles/katex-layout.css';

// 실제 서버로 갈 요청(같은 원점의 /api·/auth·/healthz, 다른 원점 전체)은 보내지 않고 실패시킨다. 데이터가 필요한
// story는 `api`의 함수를 story 안에서 바꿔 끼운다(stories/fixtures). 막힌 요청은 콘솔 오류로 남겨 찾을 수 있게 한다.
// Storybook 자신의 요청(index.json, 모듈)은 같은 원점의 다른 경로라 그대로 보낸다.
const PRODUCT_PATHS = ['/api/', '/auth', '/healthz'];
const realFetch = globalThis.fetch.bind(globalThis);
globalThis.fetch = (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
  const url = new URL(input instanceof Request ? input.url : input, window.location.href);
  if (
    url.origin !== window.location.origin ||
    PRODUCT_PATHS.some((path) => url.pathname.startsWith(path))
  ) {
    const method = init?.method ?? (input instanceof Request ? input.method : 'GET');
    console.error(`[storybook] blocked unexpected request: ${method} ${url.href}`);
    return Promise.reject(new TypeError(`blocked by Storybook: ${url.href}`));
  }
  return realFetch(input, init);
};

// 브랜치 미리보기는 push마다 같은 주소에 새 해시 파일로 덮어쓴다. 이미 열린 탭이 스토리를 바꾸다 사라진 옛 청크를
// 동적 import해 404가 나면(`vite:preloadError`) 이 미리보기 창을 다시 읽는다. 실패 묶음마다 한 번이다: 다시 읽어 뜬 직후 또
// 나면 오류를 그대로 두고(무한 다시 읽기 방지), 오래 열어 둔 뒤 다음 배포를 만나면 또 한 번 다시 읽는다(`preload-reload.ts`).
watchPreloadErrors();

/** 브라우저 글자 크기 설정을 흉내 낸다(rem 기준 크기 확인). 실제 브라우저·OS 설정 시험과는 다르다. */
const FONT_SCALES = ['100', '150', '200'];
/**
 * 글자 크기 전역값을 지원하는 배율로 정한다. 도구 막대는 문자열('200')을 주지만 주소의 `globals=fontScale:200`은
 * Storybook이 숫자(200)로 읽는다. 둘을 같은 배율로 보고, 그 밖의 값은 CSS에 넣지 않고 100%로 돌린다.
 */
function fontScaleOf(value: unknown): string {
  const text = typeof value === 'number' ? String(value) : typeof value === 'string' ? value : '';
  return FONT_SCALES.includes(text) ? text : '100';
}

const preview: Preview = {
  globalTypes: {
    locale: {
      description: '표시 언어',
      toolbar: {
        title: '언어',
        icon: 'globe',
        items: [
          { value: 'ko', title: '한국어' },
          { value: 'en', title: 'English' },
        ],
        dynamicTitle: true,
      },
    },
    fontScale: {
      description: '글자 크기(루트 글꼴 배율)',
      toolbar: {
        title: '글자 크기',
        icon: 'zoom',
        items: FONT_SCALES.map((value) => ({ value, title: `${value}%` })),
        dynamicTitle: true,
      },
    },
    theme: {
      description: '화면 밝기(앱의 표시 설정과 같은 .dark 토큰)',
      toolbar: {
        title: '밝기',
        icon: 'contrast',
        items: [
          { value: 'light', title: '밝게' },
          { value: 'dark', title: '어둡게' },
        ],
        dynamicTitle: true,
      },
    },
  },
  initialGlobals: { locale: 'ko', fontScale: '100', theme: 'light' },
  parameters: {
    layout: 'padded',
    viewport: {
      options: {
        narrow375: { name: '375px', styles: { width: '375px', height: '667px' }, type: 'mobile' },
        narrow500: { name: '500px', styles: { width: '500px', height: '800px' }, type: 'mobile' },
        desktop: {
          name: '데스크톱 1280px',
          styles: { width: '1280px', height: '800px' },
          type: 'desktop',
        },
      },
    },
  },
  async beforeEach({ globals }) {
    const locale: DisplayLocale =
      typeof globals.locale === 'string' ? (asLocale(globals.locale) ?? 'ko') : 'ko';
    const scale = fontScaleOf(globals.fontScale);
    // 로그인한 사람의 표시 설정 자리에 고정값을 둔다: 날짜·시각이 보는 컴퓨터의 시간대와 상관없이 같게 보인다.
    Object.assign(me, {
      userId: '',
      timeZone: 'Asia/Seoul',
      displayName: '',
      locale,
      weekStart: '',
      hourCycle: '',
    });
    await setLocale(locale, { reload: false });
    document.documentElement.lang = locale;
    document.documentElement.style.fontSize = `${scale}%`;
    applyDocFont('sans');
    // 보는 컴퓨터의 시스템 설정과 상관없이 고른 밝기로 고정한다("시스템 설정 따르기"는 고르지 않는다).
    applyTheme(globals.theme === 'dark' ? 'dark' : 'light');
  },
};

export default preview;
