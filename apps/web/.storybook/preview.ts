// 모든 story에 공통인 준비. 제품의 전역 스타일·서체·번역을 그대로 쓰고, 표시 설정과 네트워크를 결정적으로 고정한다.
import type { Preview } from '@storybook/svelte';

import { asLocale, type DisplayLocale } from '$lib/display';
import { applyDocFont } from '$lib/doc-font';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

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

/** 브라우저 글자 크기 설정을 흉내 낸다(rem 기준 크기 확인). 실제 브라우저·OS 설정 시험과는 다르다. */
const FONT_SCALES = ['100', '150', '200'];

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
  },
  initialGlobals: { locale: 'ko', fontScale: '100' },
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
    const scale = typeof globals.fontScale === 'string' ? globals.fontScale : '100';
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
    document.documentElement.style.fontSize = FONT_SCALES.includes(scale) ? `${scale}%` : '';
    applyDocFont('sans');
  },
};

export default preview;
