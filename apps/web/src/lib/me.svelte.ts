// 로그인한 사람(앱이 처음 읽을 때 채운다). 화면이 "나"를 가려야 할 때(예: 내 담당 업무)와 내 표시 설정(D-50 시간대,
// D-81 언어·주 시작·시각 표시, D-109 화면 밝기)에 쓴다. 표시 설정의 빈 값은 아직 고르지 않음이다(화면 밝기는 기본이 `system`).
export const me = $state({
  userId: '',
  timeZone: '',
  displayName: '',
  locale: '',
  weekStart: '',
  hourCycle: '',
  theme: 'system',
  // 서버의 제목 글자 수 한도(FVOCI_MAX_TITLE_CHARS, #79). 서버 기본값으로 시작해 로그인 읽기에서 맞춘다.
  maxTitleChars: 200,
});
