import '@testing-library/jest-dom/vitest';

import { beforeEach } from 'vitest';

import { org } from '$lib/org.svelte';
// 시험마다 새 페이지처럼 계정 전환 안전 상태를 초기화한다.
import { accountSafety } from '$lib/features/feedback/feedback.svelte';

import { TEST_ORGANIZATION } from './component/fixtures/organization';

// 개인 화면은 현재 조직 범위라 조직이 없으면 "조직을 고르세요"만 보인다(D-111). 시험은 기본으로 이 조직 하나에 속해 그것을
// 고른 상태에서 시작한다(저장소에는 쓰지 않는다). 조직 없는 상태를 보려는 시험은 `org.reset()`을 부른다. 앱 셸을 그리는
// 시험은 `api.myOrganizations`·`api.unreadCount`를 직접 spy 한다.
beforeEach(() => {
  accountSafety.switching = false;
  org.load('user-a', [TEST_ORGANIZATION], TEST_ORGANIZATION.id);
});
