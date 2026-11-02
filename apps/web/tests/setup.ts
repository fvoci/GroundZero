import '@testing-library/jest-dom/vitest';

import { afterEach, beforeEach } from 'vitest';

import { org } from '$lib/org.svelte';
// 시험마다 새 페이지처럼 계정 전환 안전 상태를 초기화한다.
import { accountSafety, stopConnectionProbeForTests } from '$lib/features/feedback/feedback.svelte';

import { TEST_ORGANIZATION } from './component/fixtures/organization';

// 개인 화면은 현재 조직 범위라 조직이 없으면 "조직을 고르세요"만 보인다(D-111). 시험은 기본으로 이 조직 하나에 속해 그것을
// 고른 상태에서 시작한다(저장소에는 쓰지 않는다). 조직 없는 상태를 보려는 시험은 `org.reset()`을 부른다. 앱 셸을 그리는
// 시험은 `api.myOrganizations`·`activityApi.unreadCount`를 직접 spy 한다.
beforeEach(() => {
  accountSafety.switching = false;
  org.load('user-a', [TEST_ORGANIZATION], TEST_ORGANIZATION.id);
});

// 연결 확인(`/healthz` 5초 간격)은 모듈 수준 타이머라 시험이 끝나도 남아 뒤 시험에서 실제 요청을 보낸다. 시험마다
// 멈춘다(#309). 파일의 정리가 가짜 시계를 먼저 풀었다면 그 시계의 타이머는 더 돌지 않고, 여기서는 확인 상태를 비워
// 뒤 시험이 끊김을 알리면 새 확인을 시작하게 한다.
afterEach(() => {
  stopConnectionProbeForTests();
});
