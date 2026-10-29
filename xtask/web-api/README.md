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

`apps/web/src`의 TS·JS·Svelte 소스에서 공통 `request`와 허용한 전달 도우미의
호출을 AST와 심벌로 찾는다. 생성 번역은 제외한다. 문자열·상수·조건식·문자열
리터럴 유니온을 풀고, `encodeURIComponent`와 숫자의 `String` 변환을 경로 한
조각으로 취급한다. 쿼리와 fragment는 작업의 경로·메서드 대조 대상이 아니다.
Svelte의 script와 마크업 표현식도 검사하며 import 별칭·재노출·namespace 호출을
따라간다. URL 상수와 `/api`·`/api/public` 접두어 검사는 호출로 세지 않는다.
경로·메서드가 정적으로 풀리지 않거나 도우미를 콜백 등으로 넘기면 실패한다.

`allowlist.json`의 두 목록은 자동으로 늘리지 않는다.

- `forwarders`: `method`, `path`를 그대로 넘기는 공통 도우미다. 파일·함수 이름,
  이유 꼬리표와 구체적 설명을 적는다. 전달 호출은 정확히 한 번이어야 하며,
  도우미의 모든 호출 지점을 따로 검사한다. 목록에 없는 동적 전달은 실패한다.
- `unused`: `request` 경로에서 호출되지 않는 OpenAPI 작업의 정확한 메서드·경로와
  이유 꼬리표다. 새 누락, 중복, 이미 쓰거나 계약에서 사라진 낡은 항목은 실패한다.
  화면을 붙이는 조각에서 해당 항목을 지운다. 꼬리표는 관리 화면 작업의
  `샴 가3~가5 조립 예정`, 나머지의 `가6 판정 예정` 두 개뿐이다.

이 검사는 요청 도우미의 정적 호출 대조이며 화면에서의 실제 도달 가능성을
증명하지 않는다. 직접 `fetch`·다운로드 링크·인증 리디렉션은 사용 작업으로 세지
않는다. 따라서 파일 내용·업로드 조각·내보내기·상태 확인처럼 이 경로 밖에 있는
작업도 가6에서 판정할 목록에 남는다. 서버 API와 OpenAPI 내용은 바꾸지 않는다.

가0(#153)가 먼저 병합되면 main을 합친 뒤 현재 계약으로 다시 검사한다. 목록은
실제로 남은 작업을 확인해 갱신한다.
