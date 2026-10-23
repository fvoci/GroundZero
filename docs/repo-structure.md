# fvoci 저장소 구조

초기 구조는 [D-46](decisions.md#d-46)에 따라 승인됐다. 파일 배치 같은 구현 선택은
[D-47](decisions.md#d-47)의 경계를 따른다. 아래는 책임 배치이며
전체 파일 목록이 아니다. 빈 폴더를 미리 만들거나 기능마다 같은 공통 코드를 복제하지 않는다.
현재 파일은 필요한 경로에 한정해 `rg --files` 또는 `git ls-files`로 확인한다.

## 코드와 계약

| 경로                                                                | 책임                                                                              |
| ------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| `crates/server/src/app.rs`, `config.rs`, `main.rs`                  | 서버 구성·시작·종료와 설정                                                        |
| `crates/server/src/http/`                                           | HTTP 라우팅·공통 경계·OpenAPI                                                     |
| `crates/server/src/db/`                                             | 연결·트랜잭션·RLS 문맥·역할 준비                                                  |
| `crates/server/migrations/`                                         | 제품 스키마·SQL 함수의 정본. [읽기와 변경](../crates/server/migrations/README.md) |
| `crates/server/src/features/`                                       | 기능별 서버 동작. 공통 권한·데이터를 재사용                                       |
| `crates/server/src/collab/`                                         | 실시간 공동 편집                                                                  |
| `crates/server/src/worker.rs`, `search.rs`, `storage/`, `backup.rs` | 백그라운드 작업·검색·파일 저장·백업과 복구                                        |
| `crates/server/tests/`                                              | 기능별 API·DB·권한 통합 시험, 공통 준비는 `common/`                               |
| `apps/web/src/app/`, `src/lib/features/`                            | 앱 시작·화면 이동과 기능별 UI                                                     |
| `apps/web/src/lib/ui/`                                              | 가져온 공통 UI. [출처·추가·수정 규칙](../apps/web/src/lib/ui/README.md)           |
| `apps/web/src/lib/api/`                                             | 생성 계약과 런타임 입력 검증을 연결하는 클라이언트                                |
| `apps/web/src/styles/`, `messages/`, `project.inlang/`              | 디자인 토큰·화면 문구·번역 구성                                                   |
| `apps/web/tests/`                                                   | 웹 단위·컴포넌트 시험                                                             |
| `contracts/openapi.json`, `packages/api-client/src/generated/`      | Rust 정의에서 생성하는 API 계약과 TypeScript 타입                                 |
| `contracts/collab/`                                                 | 실시간 공동 편집의 기준 파일                                                      |

프로젝트·취미 등의 모듈 종류는 `modules`·`records`가, 알림 화면은 `references`가 맡는다.
개인 노트는 `documents`에서 소유권·권한으로 구분하며 편집·저장 코드를 복제하지 않는다.
폴더 이름이 없다는 이유로 기능이 제외됐다고 판단하지 않는다.

## 실행·개발 도구

- `xtask/`: 개발·검사·통합 시험·API 생성·라이선스·rhwp·SQL 비교의 실행 진입점.
  사용법은 [개발 지침](development.md), 설치는 [셀프호스트 안내](self-hosting.md)를 따른다.
- `deploy/`: 개발·시험·셀프호스트 Compose와 Dockerfile, PostgreSQL·Keycloak 준비,
  Supabase 시험, rhwp 빌드 입력, CDN 예시. 서비스별 역할과 제약을 문서에서 확인한다.
- `.github/workflows/`: xtask를 실행하는 CI와 이미지 게시.
- 루트의 Cargo·pnpm·TypeScript·lint 설정: 도구 버전과 검사 설정의 정본.
- `AGENTS.md`: 공통 진입점. `.agents/skills/`는 변경 종류에 필요한 작업 지침만 둔다.

## 문서와 과거 증거

- [결정 기록](decisions.md): 승인 원칙, 미승인 제안, 보류·대체 관계.
- [개발 지침](development.md): 현재 개발·검사 방법과 유지보수 계약.
- [마이그레이션 안내](../crates/server/migrations/README.md): 제품 SQL 기준과 고정 SHA 검증 재현·과거 증거 위치.
  제품 사전 검사 fixture는 `crates/server/tests/fixtures/postgres-preflight/`에 둔다.
  통합 전용 비교 도구·준비 문서·후보 SQL 사본은 현재 트리에 두지 않는다.

과거 실행 로그·스크린숏·중간 소스는 현재 코드와 분리하고, 관련 결함이나 SHA를 조사할 때만 연다.
분리 원칙은 [개발 지침의 검사와 예외](development.md#4-검사와-예외)를 따른다.
초기 파일 트리와 미승인 검토자 메모는
[구조안 원문](https://github.com/fvoci/GroundZero/blob/6ecad60f737363dfb0ffb7cf79b623ac84871761/docs/repo-structure.md)에 남아 있다.
