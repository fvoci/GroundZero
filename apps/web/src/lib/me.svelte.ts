// 로그인한 사람(앱이 처음 읽을 때 채운다). 화면이 "나"를 가려야 할 때(예: 내 담당 업무)와 내 표시 설정(D-50 시간대,
// D-81 언어·주 시작·시각 표시)에 쓴다. 표시 설정의 빈 값은 아직 고르지 않음이다.
export const me = $state({
  userId: '',
  timeZone: '',
  displayName: '',
  locale: '',
  weekStart: '',
  hourCycle: '',
});
