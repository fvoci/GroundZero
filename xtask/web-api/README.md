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
도우미의 const 별칭·다단계 별칭·import 별칭도 함수 본문까지 따르며 순환은 멈춘다.
참조 탐색은 반복문으로 별칭 선언을 따라가는 전이를 최대 64번 허용하며, 64번째 목적지도
검사한다. 괄호·as·satisfies와 const 함수 본문은 별칭 전이로 세지 않는다. 초과하면 정적
해석 실패로 거절한다.
함수 선언 심벌에 대입·구조 분해·반복문 등의 쓰기가 있으면 원래 본문으로 판단하지 않고 거절한다.
SR-250-14의 명시 `undefined` 기본값 해석 보강은 철회했다. 생략 인자는 부모 판의 처리를
유지하지만, 명시된 비리터럴 인자가 기본값을 고르는지는 추론하지 않고 정적 해석 실패로
거절한다. 명시 리터럴은 그대로 전달하며, 새 호출은 명시 인자나 기본값 없는 도우미로 쓴다.
기본값 없는 선택 인자와 작업 경로에 영향을 주지 않는 query 전용 인자는 이 해석이 필요 없다.

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
  원점은 AST·심벌로 브라우저의 protocol·host에서 조립했는지 확인한다. 현재 `live.ts`처럼
  `location.protocol === 'https:' ? 'wss' : 'ws'`(또는 http 조건의 반대 조합)와
  `location.host`를 합친 표현식 및 const 별칭을 지원한다. bare location과 window·self·document·
  진짜 globalThis의 location을 원점·경로 평가에서 같은 심벌 판정으로 지원하며, 지역 변수나
  import/별칭으로 가린 이름은 받지 않는다. fetch·location 등 capability 판정도 이 전역
  식별 규칙을 공유한다. 외부 호스트·포트 추가·반대 scheme
  조합·정적으로 증명할 수 없는 조립은 실패한다. 가상 host 문자열과의 일치만으로 허용하지 않는다.

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

[부조종사 #251 결정](https://github.com/fvoci/GroundZero/issues/251#issuecomment-6559974613)에
따라 main `06ddabbd`의 웹 소스를 대조하고 SR-250-11(WS 원점)·13(불변 별칭)을 보강했다.
SR-250-14는 [SR-295-10 결정 (나)](https://github.com/fvoci/GroundZero/pull/295#issuecomment-6560952286)에
따라 보강을 걷어내고 (2) 한계로 되돌렸다. 현재 한계는 아래 일곱 행이다. 제품 변경과
새 의존성·출구별 예외·전수 출구 목록은 없다.
현재 검사는 화면의 실제 도달 가능성이나 브라우저의 모든 네트워크 동작을 증명하지 않는다.

| 항목                    | 현재 범위 밖인 것                                           | 한계를 유지하는 이유·현재 사용                                                                                                                                                                                                              |
| ----------------------- | ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| SR-250-8                | Svelte spread 속성으로 넘긴 URL                             | 링크 렌더링 6곳은 snippet props·restProps를 받는다. 런타임 객체의 최종 href를 일반적으로 풀 수 없다. spread만으로 href를 받는 MenuSubButton은 현재 호출 0곳이다. 명시 href 뒤 spread의 덮어쓰기도 보장하지 않는다                           |
| SR-250-9                | DOMParser·innerHTML로 만든 HTML 안 자원                     | 문서 인쇄·붙여넣기·DOCX의 3흐름은 저장 문서·사용자 HTML·라이브러리 결과를 받는다. 콘텐츠 속 URL·개수는 정적으로 열거할 수 없다. template 파싱을 실제 요청으로 세거나 CSP를 계약 검증으로 보지 않는다                                        |
| SR-250-10               | 동적 encodeURIComponent 값의 `.`·`..`                       | 명시 출구 175곳의 기반 URL 식 170개에 경로 인코딩 184개가 있다. 일반 string의 실제 점 여부를 정적으로 보장할 수 없다. UUID뿐 아니라 공개 token도 포함한다. 런타임 거절은 제품 동작 변경이므로 하지 않는다. 알려진 리터럴 점은 계속 실패한다 |
| SR-250-12               | 끝까지 풀리지 않는 링크·location 런타임 값                  | 런타임 URL 위치 32곳은 앱 경로·공용 prop·콘텐츠·정적 CSS/blob URL·로그아웃 응답 등을 쓴다. 전부 거절하면 정상 링크도 실패한다. 다른 호출의 알려진 API 리터럴 검사는 유지한다                                                                |
| SR-250-14 (2)           | 명시 undefined·비리터럴 인자에 기본값 적용                  | 회차마다 흐름 타입·mutable 값·64-hop property 캡처에서 검사 약화가 나왔다. 타입 추론 기반 기본값 해석을 걷어내 정적 해석 실패로 닫는다. 현재 해당 웹 사용은 0곳이다. 새로 쓸 때는 명시 인자나 기본값 없는 도우미로 쓴다                     |
| #251 후속 댓글          | 재대입된 let·매개변수의 `/api` 리터럴을 apiHint가 보지 않음 | 현재 해당 API 링크·이동 사용은 0곳이다. 쓰기 지점만 hint에 더하면 정상 API도 해석 불가로 거절하며, 일반 분기·클로저 흐름은 정적으로 보장할 수 없다. initializer만 읽는 한계를 유지한다                                                      |
| 현재 웹 사용 0건인 출구 | XMLHttpRequest·EventSource·navigator.sendBeacon·new Request | 각각 0곳이다. 문법별 정적 검사는 가능하지만 현재 사용이 없는 새 범위다. 미지원으로 유지하며 도입 전에 검사 범위를 먼저 결정한다                                                                                                             |

위 수는 소스 위치·URL 식·인코딩 인자의 수다. 반복 횟수나 실제 화면 요청 수가 아니며 행끼리
겹친다. SR-250-14의 첨부 링크 `fileContentUrl(f.id, undefined, true)`는 기본값 없는 version
인자에 undefined를 주므로 보강 전에도 해당 한계를 일으키는 사용은 아니었다.

이 종류를 새로 사용하면 검사 범위를 먼저 결정한다. 특히 XMLHttpRequest·sendBeacon·
EventSource의 상대 URL 거절은 현재 범위 밖이므로 구현하지 않는다.
서버 API·계약 내용은 바꾸지 않는다. main `06ddabbd`에서 현재 미사용 목록은
인증 callback 1개다. 보강 전후 모두 웹 호출 222곳·사용 작업 218개·이유 있는 미사용 1개다.
