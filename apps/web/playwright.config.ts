// 실제 브라우저 회귀 시험(#118). `cargo xtask browser`가 실제 서버·DB·테스트 OIDC를 띄우고 이 설정으로 Chromium을 돌린다.
// 시험은 HTTP를 대신 응답(mock)하지 않는다. 화면 크기 프로젝트는 viewport 시험이며 실제 휴대 기기 시험이 아니다.
import path from 'node:path';

import { defineConfig, devices } from '@playwright/test';

const baseURL = process.env['FVOCI_BROWSER_BASE_URL'];
const raw = process.env['FVOCI_BROWSER_RAW'];
if (baseURL === undefined || raw === undefined) {
  throw new Error('run through `cargo xtask browser` (FVOCI_BROWSER_BASE_URL, FVOCI_BROWSER_RAW)');
}
// 실패 증거 수집 확인용 시험은 따로 켤 때만 돈다(의도적으로 실패한다).
const smokeIgnore =
  process.env['FVOCI_BROWSER_FAILURE_SMOKE'] === undefined ? ['failure-smoke.spec.ts'] : [];

export default defineConfig({
  testDir: 'e2e',
  outputDir: path.join(raw, 'test-results'),
  // 첫 실패가 결과다. 다시 시도해서 실패를 가리지 않는다.
  retries: 0,
  // 한 서버·DB를 나눠 쓰므로 하나씩 돈다(시험마다 고유한 합성 데이터를 만든다).
  workers: 1,
  fullyParallel: false,
  forbidOnly: true,
  timeout: 60_000,
  expect: { timeout: 10_000 },
  reporter: [['list']],
  use: {
    baseURL,
    locale: 'ko-KR',
    timezoneId: 'Asia/Seoul',
    // 인증 단계의 trace·화면은 시험 도구(e2e/fixtures.ts)가 로그인 뒤에만 기록한다. 기본 수집은 끈다.
    trace: 'off',
    screenshot: 'off',
    video: 'off',
    actionTimeout: 10_000,
    navigationTimeout: 20_000,
  },
  projects: [
    {
      name: 'desktop',
      // 프로젝트의 testIgnore는 위 설정을 대신하므로 함께 적는다.
      testIgnore: [...smokeIgnore, 'layout-control.spec.ts', '**/representative.spec.ts'],
      use: { ...devices['Desktop Chrome'], viewport: { width: 1280, height: 800 } },
    },
    {
      // 375px 이하 viewport. 실제 휴대 기기·모바일 브라우저 엔진 시험이 아니다.
      name: 'narrow-375',
      // 좁은 화면이 의미 있는 배치 시험만 돈다.
      testMatch: [
        '00-representative.spec.ts',
        'draft-offer-layout.spec.ts',
        'layout.spec.ts',
        'layout-control.spec.ts',
        'gantt.spec.ts',
        'ui-dialog.spec.ts',
        'organizations.spec.ts',
      ],
      use: {
        ...devices['Desktop Chrome'],
        viewport: { width: 375, height: 812 },
        hasTouch: false,
      },
    },
  ],
});
