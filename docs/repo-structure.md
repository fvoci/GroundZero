# fvoci 저장소 구조안

작성일: 2026-10-17, 목록·트리 갱신: 2026-10-22
상태: 초기 구조 기준으로 승인됨(`docs/decisions.md` D-46). 구조는 책임 배치의 예시이며 전체 파일 목록이 아니다. 빈 폴더·파일을 미리 만들지 않고 구현에 필요한 순서대로 만든다. 아래 목록과 트리는 2026-10-22 저장소(`git ls-files`)에 있는 것만 적는다.

## 현재 존재하는 것 (2026-10-22, `git ls-files` 기준)

- 루트: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `deny.toml`, `.cargo/config.toml`, `package.json`, `pnpm-workspace.yaml`, `pnpm-lock.yaml`, `.npmrc`, `tsconfig.base.json`, `eslint.config.mjs`, `.prettierrc.json`, `.prettierignore`, `.editorconfig`, `.gitignore`, `.dockerignore`, `.env.example`, `.coderabbit.yaml`, `THIRD_PARTY_NOTICES.txt`, `AGENTS.md`, `README.md`, `CONTRIBUTING.md`
- `crates/server/`: `build.rs`, `migrations/`, `src/`(아래 트리), `tests/`(기능별 통합 테스트 `*.rs`와 `common/mod.rs`)
- `xtask/`: `src/{main,check,compose,dev,test,openapi,licenses,notices,rhwp,i18n,supabase,supabase_cloud_precheck}.rs`, `src/supabase_cloud_precheck/tests.rs`
- `apps/web/`: Vite + Svelte 5 SPA, `src/{app,assets,lib,styles}/`, `messages/`(화면 문구), `project.inlang/`, `tests/`(`component/`와 최상위 시험 파일)
- `packages/api-client/`: `src/index.ts`, `src/generated/schema.ts`(생성)
- `contracts/`: `openapi.json`(생성), `collab/`(실시간 공동 편집 기준 파일, D-70)
- `deploy/`: `compose.{dev,test,selfhost}.yaml`, `selfhost.env.example`, `docker/{dev,app}.Dockerfile`, `keycloak/realm-{test,selfhost}.json`, `postgres/`(`initdb/`와 개발 준비 SQL), `supabase/`(D-88 시험 구성·연결 예시), `rhwp/`(HWP·HWPX 미리보기 WASM 빌드 입력, D-76), `cdn/`(CDN 설정 예시, D-77)
- `.agents/skills/{rust-api-db,svelte-ui,test-docker-ci}/SKILL.md`
- `.github/workflows/{base-image,ci,release-image,supabase-cloud-readonly}.yml`
- `docs/`: `decisions.md`, `development.md`, `repo-structure.md`, `self-hosting.md`, `self-hosting-supabase.md`, `try-it.md`, `sql-id-contract.md`, `supabase-cloud-readonly-precheck.md`, `reference-products-survey.md`

프론트엔드 `features/`에 `projects`·`hobbies`·`notifications` 폴더가 없는 것은 기능 제외가 아니다. 모듈 종류(프로젝트·취미 등)는 `modules`·`records`가, 알림 화면은 `references`가 맡는다. 개인 노트는 `documents` 안에서 소유권·권한으로 구분하며 편집·저장 코드를 복제하지 않는다.

`preparation/sql-consolidation/`에는 검토 후보 SQL, 재현용 조회·fixture, SHA별 요약을 둔다. 실행별 원시 로그·카탈로그와 환경 스냅숏은 [기존 원격 보존본](../preparation/sql-consolidation/evidence.md)으로 분리했다. 그중 과거 실행 기록은 **역사 기록이며 현재 상태를 나타내지 않음**이며 현재 구현은 코드·테스트와 승인 결정을 확인한다. 새 실행 출력은 기존 `target/` 아래에 두며 원격 보존 트리를 현재 소스에 복사하지 않는다.

```
fvoci/
├── Cargo.toml                 # Rust workspace
├── Cargo.lock
├── rust-toolchain.toml
├── rustfmt.toml
├── clippy.toml                # 필요한 Clippy 설정만
├── deny.toml                  # cargo-deny(라이선스·권고)
├── package.json              # pnpm 공통 명령·버전 지정
├── pnpm-workspace.yaml
├── pnpm-lock.yaml
├── tsconfig.base.json
├── eslint.config.mjs
├── .prettierrc.json
├── .editorconfig
├── .gitignore
├── .dockerignore
├── .env.example              # 설명과 비밀 아닌 예시만
├── THIRD_PARTY_NOTICES.txt   # 배포물 제3자 고지(`cargo xtask notices`)
├── .cargo/
│   └── config.toml           # cargo xtask 별칭 등
├── AGENTS.md                 # 짧은 공통 규칙·스킬 진입점
├── README.md
├── CONTRIBUTING.md
│
├── crates/
│   └── server/
│       ├── Cargo.toml
│       ├── build.rs
│       ├── migrations/       # PostgreSQL 스키마 변경의 정본
│       ├── src/
│       │   ├── main.rs
│       │   ├── lib.rs
│       │   ├── config.rs
│       │   ├── app.rs        # 구성·시작·종료 연결
│       │   ├── error.rs
│       │   ├── build_info.rs # 제품 버전·소스 SHA(D-87)
│       │   ├── ids.rs        # 의미 있는 ID·역할 타입(D-09)
│       │   ├── logging.rs    # 운영 로그(D-83)
│       │   ├── http/         # 라우팅·HTTP 공통 경계·캐시 머리글
│       │   ├── db/           # 풀·트랜잭션·RLS 문맥, 역할 준비
│       │   ├── worker.rs     # 백그라운드 작업(O-19, 실행 역할로 켬)
│       │   ├── search.rs     # 통합 검색 색인·Meilisearch 연결(D-39)
│       │   ├── storage/      # 로컬·S3 구현
│       │   ├── backup.rs     # 백업·복구
│       │   ├── collab/       # 실시간 공동 편집(D-70)
│       │   └── features/     # 기능마다 .rs 파일 하나(하위 폴더 없음), mod.rs가 묶는다
│       │       ├── accounts.rs · workspaces.rs · members.rs · audit.rs
│       │       ├── modules.rs · item_profile.rs · records.rs · zotero.rs
│       │       ├── documents.rs · revisions.rs · references.rs · comments.rs · public_links.rs
│       │       ├── tasks.rs · schedule.rs · recurrence.rs · events.rs · saved_views.rs
│       │       ├── time_tracking.rs · courses.rs · study.rs · home.rs
│       │       ├── files.rs · file_text.rs · export.rs · trash.rs
│       │       └── search_api.rs · search_text.rs
│       └── tests/            # API·DB·권한 통합 테스트(기능별 *.rs)
│           └── common/
│
├── apps/
│   └── web/
│       ├── package.json
│       ├── tsconfig.json
│       ├── vite.config.ts
│       ├── vite.pptx-frame.config.ts
│       ├── svelte.config.js
│       ├── messages/         # 화면 문구(ko·en, D-81)
│       ├── project.inlang/
│       ├── src/
│       │   ├── app/          # 시작·화면 이동·전체 설정
│       │   ├── assets/
│       │   ├── lib/
│       │   │   ├── api/      # 생성 client 연결 설정
│       │   │   ├── ui/       # 공통 UI
│       │   │   ├── hooks/
│       │   │   └── features/ # 기능별 폴더. 편집기(Tiptap)는 documents, 미리보기(Office·HWP)는 files 안
│       │   │       ├── workspaces/ · members/ · settings/ · nav/ · auth/ · health/
│       │   │       ├── modules/ · records/ · courses/ · study/
│       │   │       ├── documents/ · references/ · comments/ · notes/ · public/
│       │   │       ├── tasks/ · checklists/ · events/ · time/ · home/
│       │   │       └── files/ · search/ · trash/ · audit/ · feedback/
│       │   └── styles/
│       └── tests/
│           └── component/
│
├── packages/
│   └── api-client/           # 계약에서 생성하는 TS client
│       ├── package.json
│       └── src/generated/
│
├── contracts/
│   ├── openapi.json          # Rust 정의에서 생성
│   └── collab/               # 실시간 공동 편집 기준 파일(D-70)
│
├── xtask/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── check.rs
│       ├── compose.rs
│       ├── dev.rs
│       ├── test.rs
│       ├── openapi.rs
│       ├── licenses.rs
│       ├── notices.rs
│       ├── i18n.rs
│       ├── rhwp.rs
│       ├── supabase.rs
│       └── supabase_cloud_precheck.rs
│
├── deploy/
│   ├── docker/
│   │   ├── dev.Dockerfile
│   │   └── app.Dockerfile
│   ├── compose.dev.yaml
│   ├── compose.test.yaml
│   ├── compose.selfhost.yaml   # 셀프호스트(docs/self-hosting.md)
│   ├── selfhost.env.example
│   ├── postgres/              # 첫 설치 때 Keycloak DB(initdb/, 선택), 개발 준비 SQL
│   ├── keycloak/              # 테스트 렐름, 셀프호스트 렐름 틀
│   ├── supabase/              # 셀프호스트 Supabase 시험 구성·연결 예시(D-88)
│   ├── rhwp/                  # HWP·HWPX 미리보기 WASM 빌드 입력(D-76)
│   └── cdn/                   # Cloudflare·CloudFront 설정 예시(D-77)
│
├── docs/
│   ├── decisions.md
│   ├── development.md
│   ├── repo-structure.md
│   ├── self-hosting.md
│   ├── self-hosting-supabase.md
│   ├── try-it.md               # 이 Mac에서 직접 써 보기(로컬 체험)
│   ├── sql-id-contract.md
│   ├── supabase-cloud-readonly-precheck.md
│   └── reference-products-survey.md
│
├── .agents/
│   └── skills/              # 실제로 필요한 역할별 스킬
└── .github/
    └── workflows/           # 공통 검사 명령 실행과 gate, 이미지 게시
```

## 검토자 메모 (제안, 미승인)

구조 기준 자체는 승인되었다. 아래는 초기화 시 D-47에 따라 검토자가 판단하고 이유를 기록할 사항이다.

- 결정 기록과 맞는 점: `contracts/openapi.json`을 Rust 정의에서 생성하고 `packages/api-client`를 그 계약에서 생성하는 구조는 O-22 권장안과 일치한다. 백그라운드 작업은 O-19 권장안대로 `crates/server/src/worker.rs`(단일 바이너리, 실행 역할, PostgreSQL 기록)로 두었다. `features/documents/`가 개인 노트까지 담는 것은 O-02(개인 노트 소유 구조)의 결과에 따라 분리될 수 있다.
- 프론트엔드 `features/`에 `projects`, `hobbies`, `notifications`, `audit`, `modules`가 없다. 서버 쪽에는 있으므로 의도적 생략인지 확인이 필요하다. (2026-10-22: `audit`·`modules`는 생겼고, 나머지는 위 설명대로 다른 폴더가 맡는다.)
- `deploy/compose.test.yaml`에 Keycloak이 들어가야 한다(D-45, D-10). `compose.selfhost.yaml`의 Keycloak은 선택 프로필(O-25 해소).
- `docs/development.md`는 기존 `development-guidelines.md`를 이 이름으로 옮겼다. `product.md`, `architecture.md`, `testing.md`, `security.md`는 아직 없다(`self-hosting.md`는 생겼다).
- `AGENTS.md`와 `.agents/skills/`의 역할과 내용은 별도 지침이 필요하다.
