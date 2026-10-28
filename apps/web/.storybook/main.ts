// Storybook: 실제 제품 컴포넌트의 상태를 합성 데이터로 보는 로컬 작업 환경(D-108 docs/decisions/ui.md#d-108).
// 앱의 vite.config.ts(Svelte·Tailwind·$lib 별칭)를 그대로 쓰고, 실제 서버로 넘기는 개발 프록시만 뺀다.
import type { StorybookConfig } from '@storybook/svelte-vite';

const config: StorybookConfig = {
  framework: '@storybook/svelte-vite',
  stories: ['../stories/**/*.stories.ts'],
  // 처음에는 Storybook 기본 기능(조작값·동작 기록·화면 크기·상호작용)만 쓴다. 추가 addon 없음.
  addons: [],
  core: {
    // 외부로 보내는 사용 통계·오류 보고·새 소식 확인을 끈다(외부 업로드 없음).
    disableTelemetry: true,
    enableCrashReports: false,
    disableWhatsNewNotifications: true,
    // 정적 빌드에 사용 통계용 project.json(사용자별 시각 포함)을 넣지 않는다. 빌드는 공개 배포된다(D-108).
    disableProjectJson: true,
  },
  viteFinal(viteConfig) {
    // 개발 서버의 /api·/auth·/healthz 프록시를 지운다. story는 실제 서버에 닿지 않는다(preview.ts의 요청 차단).
    return { ...viteConfig, server: { ...viteConfig.server, proxy: {} } };
  },
};

export default config;
