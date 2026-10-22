---
name: rust-api-db
description: Rust 서버(crates/server)·HTTP API·PostgreSQL 스키마·권한 검사 코드를 추가하거나 바꿀 때 읽는다. 프론트엔드만 바꿀 때는 읽지 않는다.
---

# Rust·API·DB 변경 지침

적용 대상: `crates/server/**`, `xtask/**`, `contracts/openapi.json`, `deny.toml`, `Cargo.toml`.

## 코드 규칙 (docs/development.md 1절)

- 제품 코드에서 `unwrap`·`expect`·`panic!`·`todo!`를 쓰지 않는다. 워크스페이스 lint가 거부한다. 오류는 `Result`와 `error::AppError`로 전달한다.
- `unsafe`는 `forbid`다. 필요하면 먼저 보고한다.
- ID·권한·상태·단위는 전용 타입으로 표현한다. 비밀 값은 `config::DatabaseUrl`처럼 `Debug`에서 가린다.
- async: spawn한 작업의 취소·오류 회수 책임을 정한다. 잠금을 잡은 채 await하지 않는다(`await_holding_lock` deny).

## API 변경

1. 핸들러에 `#[utoipa::path]`와 `ToSchema`를 붙이고 `http::ApiDoc`에 등록한다.
2. `cargo xtask openapi`로 `contracts/openapi.json`을 재생성한다. `cargo xtask check`가 최신 여부를 검사한다.
3. `pnpm run generate:api`로 TS 타입을 재생성한다. `cargo xtask check`가 커밋된 `schema.ts`가 계약에서 만든 결과와 같은지도 검사한다. 생성 타입은 런타임 검증을 대신하지 않는다.
4. 전송 표현: UUID·시각(RFC 3339 UTC)·날짜(YYYY-MM-DD)는 문자열, 64비트 정수는 number로 보내지 않는다.

## DB 변경

- 스키마의 정본은 `crates/server/migrations/`다. 적용된 마이그레이션은 고치지 말고 새 번호로 추가한다. 역할 준비는 `fvoci-server db bootstrap`, 적용은 `db migrate`.
- 앱 역할이 직접 다루면 안 되는 불변식(번호 할당, 리비전 증가, 감사 기록, 세션)은 정의자 함수·트리거로 둔다. 정의자 함수는 `SET search_path = fvoci, pg_temp`, `REVOKE ... FROM PUBLIC`, `GRANT EXECUTE TO fvoci_app`. RLS는 `app_user_id()`가 NULL이면 거부되게 쓴다(`docs/development.md` 12절).
- 모든 행에 소유 워크스페이스 또는 소유 사용자를 기록한다. RLS와 앱 권한 검사를 함께 둔다(결정 D-08, O-09).
- `sqlx::query!` 매크로는 검사 단계에 DB가 필요하므로 쓰지 않는다. `sqlx::query`와 명시 타입을 쓴다.
- 통합 테스트는 `crates/server/tests/`에 두고 `DATABASE_URL`이 없으면 실패하게 한다. `cargo xtask test`가 주입한다.

## 권한

- 멘션·링크·검색 결과·알림은 원본 접근 권한을 따른다.
- 워크스페이스 관리자는 그 공간의 비공개 문서를 읽을 수 있지만 개인 노트는 읽을 수 없다(D-40, D-49).
- 권한 확대가 필요한 변경은 구현 전에 보고한다.

## 검사

`cargo xtask check` 통과 후 `cargo xtask test`. 의존성을 추가했으면 `cargo xtask licenses` 결과를 보고에 포함한다.
