---
name: svelte-ui
description: Svelte 컴포넌트·TypeScript 클라이언트 코드(apps/web, packages/api-client)를 추가하거나 바꿀 때 읽는다. 서버만 바꿀 때는 읽지 않는다.
---

# Svelte·TypeScript·UI 변경 지침

적용 대상: `apps/web/**`, `packages/api-client/**`, `eslint.config.mjs`, `tsconfig.base.json`.

## 타입과 lint (docs/development.md 2절)

- `strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes` 등은 `tsconfig.base.json`에 켜져 있다. 끄지 않는다.
- `any`, `as unknown as T`, non-null `!`, `@ts-ignore`, `@ts-nocheck`는 ESLint가 거부한다. 타입 단언(`as`)도 금지다. 좁히기는 `in`·`typeof`·`instanceof`로 한다.
- 외부 입력(API 응답·스토리지·메시지)은 `unknown`으로 받아 실제 형태를 확인한다. 예: `apps/web/src/lib/api/client.ts`의 `parseHealth`.
- Promise는 반드시 처리한다(`no-floating-promises`). `void`를 붙이는 것은 처리가 아니다.

## 컴포넌트 수명주기

- `onMount`에서 등록한 이벤트·구독·타이머·편집기 인스턴스는 반환한 정리 함수에서 해제한다.
- 비동기 응답은 컴포넌트가 사라진 뒤 도착할 수 있다. 취소 플래그로 오래된 응답을 버린다. 예: `HealthStatus.svelte`.

## 접근성

- 상태 변화 영역에는 `aria-live`를 둔다. 버튼·링크는 실제 요소를 쓴다. 키보드로 도달할 수 없는 조작을 만들지 않는다.

## API 타입

- `@fvoci/api-client`의 타입은 `contracts/openapi.json`에서 생성한다. 손으로 고치지 않는다. 서버가 바뀌면 `pnpm run generate:api`.

## 테스트와 검사

- 컴포넌트 테스트는 `apps/web/tests/component/*.test.ts`, vitest + jsdom + Testing Library.
- `cargo xtask check`가 `pnpm run format:check`, `lint`, `check`(svelte-check), `test`를 돌린다. 개별 실행도 같은 스크립트를 쓴다.
