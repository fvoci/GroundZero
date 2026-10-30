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

- `forwarders`: method와 path를 그대로 전달하는 도우미의 파일·이름·이유.
  앞 두 매개변수에 대입·증감·구조 분해 재대입이 있으면 실패하며, 전달은 정확히
  한 번이어야 한다. 모든 호출 지점을 별도로 검사한다.
- `unused`: 웹이 부르지 않는 계약 작업의 정확한 메서드·경로·이유.
  목록 밖 누락, 중복, 이미 사용하거나 계약에서 사라진 낡은 항목은 실패한다.
  이유 꼬리표는 `샴 가3~가5 조립 예정`과 `가6 판정 예정` 두 개뿐이다.
- `websocket`: 이름 있는 협업 경로 한 항목 `document_collaboration`.
  정확한 경로 `/api/documents/{document_id}/collab`와 이유를 적는다. D-27·D-70의
  CRDT upgrade는 HTTP OpenAPI에 없으므로 이 항목과 대조한다. y-websocket의
  base와 room을 합쳐 검사하며, 항목이 없거나 경로가 다르거나 쓰이지 않으면 실패한다.

## 검사하지 않는 것

아래 한계는 후속 [#251](https://github.com/fvoci/GroundZero/issues/251)에서 가6과 함께 다시 본다.
현재 검사는 화면의 실제 도달 가능성이나 브라우저의 모든 네트워크 동작을 증명하지 않는다.

| 항목                    | 현재 범위 밖인 것                                                                        |
| ----------------------- | ---------------------------------------------------------------------------------------- |
| SR-250-8                | Svelte spread 속성으로 넘긴 URL                                                          |
| SR-250-9                | DOMParser·innerHTML로 만든 HTML 안 자원                                                  |
| SR-250-10               | 동적 encodeURIComponent 값의 `.`·`..` (현재 값은 서버가 발급한 UUID)                     |
| SR-250-11               | WebSocket 원점 (현재는 같은 원점)                                                        |
| SR-250-12               | 호출 인자를 따라가도 끝까지 풀리지 않는 링크·location의 런타임 값(앱 경로·사용자 콘텐츠) |
| 현재 웹 사용 0건인 출구 | XMLHttpRequest·EventSource·navigator.sendBeacon·new Request                              |

이 종류를 새로 사용하면 검사 범위를 먼저 결정한다. 특히 XMLHttpRequest·sendBeacon·
EventSource의 상대 URL 거절은 현재 범위 밖이므로 구현하지 않는다.
서버 API·계약 내용은 바꾸지 않는다. 현재 미사용 목록은 관리 화면 예정 16개와
인증 callback 1개, 총 17개다.
