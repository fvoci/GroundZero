// 로그인한 사람(앱이 처음 읽을 때 채운다). 화면이 "나"를 가려야 할 때(예: 내 담당 업무)와 내 시간대(D-50)에 쓴다.
export const me = $state({ userId: '', timeZone: '' });
