# 웹 API 계약 검사

`cargo xtask check`의 `web API paths and methods match OpenAPI` 단계다(D-131,
[#245 가1](https://github.com/fvoci/GroundZero/issues/245#issuecomment-6508581381)).
새 의존성 없이 저장소에 고정된 TypeScript·Svelte 컴파일러를 쓴다.

단독 실행:

```sh
node --test xtask/web-api/check.test.mts
pnpm exec tsc -p xtask/web-api/tsconfig.json
node xtask/web-api/check.mts
```

`cargo xtask check`의 새 단계도 위 세 명령을 실행한다. 시험은 틀린 경로와
메서드로 CLI를 실행해 각각 종료 코드 1을 확인한다. Node 의존성을 설치하지 않는
서버 통합 시험 job의 Rust 단위 시험에는 이 시험을 넣지 않는다.

`apps/web/src`의 TS·JS·Svelte·CSS와 `apps/web/index.html`에서 아래 출구를
AST와 심벌로 찾는다. 생성 번역은 제외한다. 문자열·상수·조건식·문자열
리터럴 유니온을 풀고, `encodeURIComponent`와 숫자의 `String` 변환을 경로 한
조각으로 취급한다. 쿼리와 fragment는 작업의 경로·메서드 대조 대상이 아니다.
Svelte의 script와 마크업 표현식도 검사하며 import 별칭·재노출·namespace 호출을
따라간다. URL 상수와 `/api`·`/api/public` 접두어 검사는 호출로 세지 않는다.
경로·메서드가 정적으로 풀리지 않거나 도우미를 콜백 등으로 넘기면 실패한다.

`fetch`는 `/api`·`/auth`·`/healthz` 아래 경로와 `options.method`를 대조한다.
메서드가 없으면 GET이다. 객체 상수·spread의 순서와 `fileContentUrl` 같은 경로 함수의
반환식·인자를 풀며, Svelte에서 함수를 import해 호출한 경우도 포함한다.
`fetchImpl: typeof fetch = fetch` 같은 기본값 매개변수, const 별칭·구조 분해,
window·globalThis 멤버도 추적한다. 해석할 수 없는 fetch 참조·재대입은 실패한다.
URL 헬퍼의 변수 initializer는 const일 때만 따라간다(CodeRabbit #250).

경로를 완성한 뒤 자원을 분류한다. 점 세그먼트(`.`·`..`), 인코딩 점·구분자,
역슬래시를 거절해 `/rhwp/../api/...`가 정적 자원으로 빠지지 않게 한다.
정적으로 풀리지 않는 자원 URL도 실패한다. `/rhwp/*` 같은 정적 자원과 외부 URL은
작업 사용 수에서 제외하되 이유 있는 예외로 출구 목록에 남긴다. 공통 request의
내부 fetch는 전달을 확인하고 중복으로 세지 않는다.

`allowlist.json`의 두 목록은 자동으로 늘리지 않는다.

- `forwarders`: `method`, `path`를 그대로 넘기는 공통 도우미다. 파일·함수 이름,
  이유 꼬리표와 구체적 설명을 적는다. 전달 호출은 정확히 한 번이어야 하며,
  도우미의 모든 호출 지점을 따로 검사한다. 메서드·경로 매개변수에 대입·증감·
  구조 분해 재대입이 있으면 실패한다. 중첩 함수의 같은 매개변수 쓰기도 잡고,
  이름만 같은 다른 변수는 구분한다. 목록에 없는 동적 전달은 실패한다.
- `unused`: 모든 대조 출구에서 호출되지 않는 OpenAPI 작업의 정확한 메서드·경로와
  이유 꼬리표다. 새 누락, 중복, 이미 쓰거나 계약에서 사라진 낡은 항목은 실패한다.
  화면을 붙이는 조각에서 해당 항목을 지운다. 꼬리표는 관리 화면 작업의
  `샴 가3~가5 조립 예정`, 나머지의 `가6 판정 예정` 두 개뿐이다.

`network-exceptions.json`은 미사용 목록과 별개다. WebSocket은 HTTP OpenAPI에 없으므로
정확한 경로(`/api/documents/{document_id}/collab`)와 이유를 적는다. y-websocket의
base와 room을 합쳐 대조하므로 방 이름이 틀려도 실패한다. 동적 문서 링크·앱 라우터·
생성 HTML 등은 파일·출구 종류·표현식이 일치하는 39개 규칙에만 예외를 적용한다.
이유가 비었거나 더 이상 쓰지 않는 예외는 실패한다. 예외를 추가·변경할 때는 해당
콘텐츠의 출처와 전달 경계를 읽고 이유를 적는다. 고정 계약 URL은 먼저 GET/메서드
대조를 하므로 이 규칙이 틀린 리터럴 경로를 숨기지 않는다.

검사가 PASS하면 `target/web-api/network-exits.json`에 모든 지점의 파일·줄·출구 종류·
표현식·분류·이유·작업을 쓴다. 아래는 SR-250-3~5 수정 시 앱 소스 전체를 조사한
기준판이다. 0인 종류도 삭제하지 않고, 새 사용은 같은 검사로 대조한다.

| 네트워크 출구               | 지점 수 | 대조함 | 이유 있는 예외 | 처리와 이유                                                             |
| --------------------------- | ------: | -----: | -------------: | ----------------------------------------------------------------------- |
| request                     |     192 |    192 |              0 | 경로·메서드 대조                                                        |
| request forwarders          |       3 |      3 |              0 | 쓰기 없는 전달을 확인하고 호출 지점 대조                                |
| 공통 fetch transport        |       1 |      1 |              0 | request URL·메서드 전달 확인                                            |
| fetch·별칭·기본값 매개변수  |       7 |      5 |              2 | GET 기본값·options.method; rhwp·PPTX 정적 자원 2개 제외                 |
| new Request                 |       0 |      0 |              0 | 생성 URL·method 및 fetch의 옵션 재정의 대조                             |
| XMLHttpRequest              |       0 |      0 |              0 | 생성자 추적 후 open(method, url) 대조                                   |
| EventSource                 |       0 |      0 |              0 | GET 대조                                                                |
| WebSocket·WebsocketProvider |       1 |      0 |              1 | CRDT upgrade 경로의 명시적 예외, base+room 대조                         |
| navigator.sendBeacon        |       0 |      0 |              0 | POST 대조                                                               |
| href 속성·DOM 쓰기          |      40 |      8 |             32 | 계약 링크 GET; 앱 경로·본문/사용자 링크·blob·공용 부품은 이유 있는 예외 |
| src 속성·DOM 쓰기           |       2 |      0 |              2 | HTML의 Vite 진입 모듈과 HWP blob 이미지                                 |
| action 속성                 |       0 |      0 |              0 | 폼 method(없으면 GET)로 대조                                            |
| location 이동·재읽기        |       7 |      1 |              6 | 내보내기 GET; replace 주입 경계 1개·현재 문서 reload 5개 예외           |
| window.open                 |       0 |      0 |              0 | GET 대조                                                                |
| action 없는 form            |      40 |      0 |             40 | 현재 앱 화면 폼; 새 API URL 구성 없음, 이벤트 핸들러의 호출은 따로 대조 |
| srcdoc·innerHTML            |       5 |      0 |              5 | 생성 미리보기·인쇄·수식·inert 붙여넣기 콘텐츠의 명시적 예외             |
| 동적 setAttribute           |       1 |      0 |              1 | 첨부 이미지 HTMLAttributes 전달, 명시적 예외                            |
| Worker                      |       1 |      0 |              1 | Vite가 번들에 넣는 HWP Worker, 명시적 예외                              |
| 동적 import                 |       5 |      0 |              5 | 패키지 4개·검증한 rhwp 해시 디렉터리의 JS 1개                           |
| WASM 로더                   |       1 |      0 |              1 | 검증한 rhwp 해시 디렉터리의 WASM, 명시적 예외                           |
| CSS url·import              |       8 |      0 |              8 | Vite가 해석하는 서체 6개·패키지 2개; 절대 API 주소는 GET 대조           |
| 합계                        |     314 |    210 |            104 | API 대조 지점 206개 + 전달 검증 4개                                     |

정적 호출·이동 대조는 화면의 실제 도달 가능성이나 런타임 본문 콘텐츠의 URL을
증명하지 않는다. 그러한 콘텐츠는 위 예외의 출처·범위로 한정한다. 현재 미사용은
관리 화면 예정 16개와 인증 callback 1개, 총 17개다. 서버 API·계약 내용은 바꾸지 않는다.

가0(#153)가 먼저 병합되면 main을 합친 뒤 현재 계약으로 다시 검사한다. 목록은
실제로 남은 작업을 확인해 갱신한다.
