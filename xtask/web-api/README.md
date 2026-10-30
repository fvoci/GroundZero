# 웹 API 계약 검사

`cargo xtask check`의 `web API paths and methods match OpenAPI` 단계다(D-131,
[#245 가1](https://github.com/fvoci/GroundZero/issues/245#issuecomment-6508581381)).
범위는 [PR #250 부조종사 판정(09:10)](https://github.com/fvoci/GroundZero/pull/250#issuecomment-6511465992)을 따른다.
새 의존성 없이 저장소에 고정된 TypeScript·Svelte 컴파일러를 쓴다.

단독 실행:

```sh
node --test xtask/web-api/check.test.mts
pnpm exec tsc -p xtask/web-api/tsconfig.json
node xtask/web-api/check.mts
```

`cargo xtask check`도 위 세 명령을 실행한다. 시험은 틀린 경로·메서드와 정적으로
풀리지 않는 호출이 실패하는지 확인한다. Node 의존성을 설치하지 않는 서버 통합
시험 job의 Rust 단위 시험에는 이 시험을 넣지 않는다.

## 검사하는 것

`apps/web/src`의 TS·JS·Svelte 및 `apps/web/index.html`에서 request·forwarder·
직접 fetch(별칭·기본값 매개변수 포함)·링크·location·WebSocket을 읽는다.
`/api`·`/auth`·`/healthz`를 가리키는 HTTP 출구만 OpenAPI의 경로·메서드와 대조한다.
링크·이동은 GET, fetch는 options.method(없으면 GET)를 쓴다. 공통 request 안의
fetch는 URL·메서드 전달을 확인하고 중복으로 세지 않는다.

AST·심벌로 import 별칭·재노출·const 별칭을 따라간다. 리터럴·상수·조건식·문자열
유니온·경로 함수의 반환식을 풀고, 인코딩한 경로 조각과 숫자의 String 변환을
처리한다. 쿼리·fragment는 작업 경로에서 제외한다. URL 상수나 접두어 검사는
호출로 세지 않는다. 대상 API 호출을 정적으로 풀 수 없으면 실패한다.
URL 헬퍼 변수의 initializer는 const일 때만 따른다. const 화살표 함수·함수 표현식의
fetch 기본값도 호출 지점에서 다른 구현으로 덮어쓰지 않았는지 확인한다.
링크·location의 매개변수는 호출 지점의 인자와 기본값을 따라 풀며, 호출 지점마다
정적으로 풀린 URL을 검사한다. 런타임 인자가 있어도 다른 호출의 리터럴을 건너뛰지 않는다.

알려진 점 세그먼트(`.`·`..`)·인코딩 점·구분자·역슬래시를 거절해
`/rhwp/../api/...`가 정적 자원으로 빠지지 않게 한다.
계약 밖 목적지(앱 경로·정적 자원·외부 URL)는 검사 대상이 아니다.
브라우저 네트워크 출구의 전수 목록·분류·사이트별 예외는 유지하지 않는다.

`allowlist.json`에는 다음을 둔다. 자동으로 항목을 늘리지 않는다.

- `forwarders`: method와 path를 그대로 전달하는 도우미의 파일·이름·이유·상세 이유(`detail`).
  앞 두 매개변수에 대입·증감·구조 분해 재대입이 있으면 실패하며, 전달은 정확히
  한 번이어야 한다. 모든 호출 지점을 별도로 검사한다.
- `unused`: 웹이 부르지 않는 계약 작업의 정확한 메서드·경로·이유.
  목록 밖 누락, 중복, 이미 사용하거나 계약에서 사라진 낡은 항목은 실패한다.
- `websocket`: 이름 있는 협업 경로 한 항목 `document_collaboration`.
  정확한 경로 `/api/documents/{document_id}/collab`와 이유를 적는다. D-27·D-70의
  CRDT upgrade는 HTTP OpenAPI에 없으므로 이 항목과 대조한다. y-websocket의
  base와 room을 합쳐 검사하며, 항목이 없거나 경로가 다르거나 쓰이지 않으면 실패한다.

`forwarders`·`unused`의 이유 꼬리표는 다음 두 개뿐이다.

- `샴 가3~가5 조립 예정`: 아직 연결하지 않은 관리 화면 작업.
- `설계상 직접 호출하지 않음`: OIDC 제공자가 브라우저를 돌려보내는 콜백이나,
  작업을 직접 지정하지 않고 호출자의 method·path를 전달하는 도우미 내부 호출.
  이 꼬리표에는 공백뿐인 문자열이 아닌 상세 이유(`detail`)가 필수다.
  전달 도우미는 꼬리표와 관계없이 항상 상세 이유가 필요하다.

`가6 판정 예정`은 폐기했으며 쓰면 실패한다. 가6에서 `/auth/callback`은
`LoginScreen.svelte:18`·`FeedbackHost.svelte:183`의 `/auth/login` 진입 뒤
`accounts.rs:77`·`:171`이 설정한 OIDC redirect_uri로 제공자가 돌려보내는 경로임을
확인했다. 인증 코드 교환·세션 발급·원래 화면 복귀에 필요하므로 영구 예외로 남긴다.
전달 도우미 세 개도 상세 이유를 유지하고 새 꼬리표로 옮겼다.
새 꼬리표는 특정 작업의 판정 이유만 나타낸다. 목록 밖 미사용은 계속 실패하며,
정확한 메서드·경로 대조, 중복·낡은 예외 거절과 전달 도우미 호출자 검사를 유지한다.

## 검사하지 않는 것

아래 한계는 [#251](https://github.com/fvoci/GroundZero/issues/251) 본문과 후속 댓글을
가6에서 현재 코드(main `093ecd0d`, 2026-10-30)와 다시 대조했다. 검사 범위는 그대로다.
현재 검사는 화면의 실제 도달 가능성이나 브라우저의 모든 네트워크 동작을 증명하지 않는다.

| 항목                    | 현재 범위 밖인 것                                           | 가6 재확인                                                                                                              |
| ----------------------- | ----------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| SR-250-8                | Svelte spread 속성으로 넘긴 URL                             | sidebar 부품이 spread를 유지한다. AppSidebar는 앱 경로를 명시적으로 넘긴다                                              |
| SR-250-9                | DOMParser·innerHTML로 만든 HTML 안 자원                     | 문서 내보내기 외에 붙여넣기 정리와 Office 미리보기 HTML도 있다                                                          |
| SR-250-10               | 동적 encodeURIComponent 값의 `.`·`..`                       | 자원 ID는 UUID다. 공개 링크의 token도 있으며 UUID만이라는 종전 설명은 부정확하다. 발급 토큰은 URL-safe base64다         |
| SR-250-11               | WebSocket 원점                                              | live.ts가 window.location의 protocol·host로 같은 원점을 만든다                                                          |
| SR-250-12               | 끝까지 풀리지 않는 링크·location 런타임 값                  | 앱 경로·사용자 콘텐츠·로그아웃 응답의 이동 주소가 있다                                                                  |
| SR-250-13               | 링크·location 도우미의 불변 별칭 호출 지점                  | callable은 함수 선언·const 함수 본문만 인식한다. 현재 이동 출구에서 해당 별칭 사용은 확인되지 않았다                    |
| SR-250-14               | 정적 `undefined` 인자에 매개변수 기본값 적용                | 첨부 미리보기 링크에도 undefined 인자가 있다. 해당 version 매개변수는 기본값 없는 선택 인자라 이 한계를 일으키지 않는다 |
| #251 후속 댓글          | 재대입된 let·매개변수의 `/api` 리터럴을 apiHint가 보지 않음 | initializer만 읽는 한계가 남는다. 현재 링크·이동 출구에서 이 재대입으로 API 주소를 만드는 사용은 확인되지 않았다        |
| 현재 웹 사용 0건인 출구 | XMLHttpRequest·EventSource·navigator.sendBeacon·new Request | apps/web/src에서 여전히 0건이다                                                                                         |

이 종류를 새로 사용하면 검사 범위를 먼저 결정한다. 특히 XMLHttpRequest·sendBeacon·
EventSource의 상대 URL 거절은 현재 범위 밖이므로 구현하지 않는다.
서버 API·계약 내용은 바꾸지 않는다. 가3 조직 화면의 11개와 가4 공간 한도의 2개
작업을 연결한 뒤 현재 미사용 목록은 가5 서버 관리 예정 3개와 인증 callback 1개,
총 4개다.
