# fvoci 저장소 구조안

작성일: 2026-10-17
상태: 초기 구조 기준으로 승인됨(`docs/decisions.md` D-46). 구조는 책임 배치의 예시이며 전체 파일 목록이 아니다. 빈 폴더·파일을 미리 만들지 않고 구현에 필요한 순서대로 만든다. 2026-10-17 초기화로 아래 중 일부만 존재한다.

## 2026-10-17 현재 존재하는 것

- 루트: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `deny.toml`, `.cargo/config.toml`, `package.json`, `pnpm-workspace.yaml`, `pnpm-lock.yaml`, `.npmrc`, `tsconfig.base.json`, `eslint.config.mjs`, `.prettierrc.json`, `.prettierignore`, `.editorconfig`, `.gitignore`, `.dockerignore`, `.env.example`, `AGENTS.md`, `README.md`, `CONTRIBUTING.md`
- `crates/server/`: `src/{main,lib,config,app,error}.rs`, `src/http/mod.rs`, `src/db/mod.rs`, `tests/health.rs`. `migrations/`와 `features/`는 첫 사용자 흐름에서 만든다.
- `xtask/`: `src/{main,check,compose,dev,test,openapi,licenses}.rs`
- `apps/web/`: Vite + Svelte 5 SPA, `src/app/`, `src/lib/{api,ui}/`, `src/styles/`, `tests/component/`
- `packages/api-client/`: `src/index.ts`, `src/generated/schema.ts`(생성)
- `contracts/openapi.json`(생성)
- `deploy/`: `compose.dev.yaml`, `compose.test.yaml`, `keycloak/realm-test.json`, `docker/{dev,app}.Dockerfile`. `compose.selfhost.yaml`·`selfhost.env.example`·`postgres/initdb/`·`keycloak/realm-selfhost.json`(O-25 해소 뒤 추가).
- `.agents/skills/{rust-api-db,svelte-ui,test-docker-ci}/SKILL.md`
- `.github/workflows/{base-image,ci}.yml`
- `docs/`: `decisions.md`, `development.md`, `repo-structure.md`. 나머지 문서는 실제 내용이 생길 때 만든다.

프론트엔드 `features/`에 `projects`·`hobbies`·`notifications`·`audit`·`modules`가 없는 것은 기능 제외가 아니다. 구현할 때 추가한다. 개인 노트는 `documents` 안에서 소유권·권한으로 구분하며 편집·저장 코드를 복제하지 않는다.

```
fvoci/
├── Cargo.toml                 # Rust workspace
├── Cargo.lock
├── rust-toolchain.toml
├── rustfmt.toml
├── clippy.toml                # 필요한 Clippy 설정만
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
├── .cargo/
│   └── config.toml           # cargo xtask 별칭 등
├── AGENTS.md                 # 짧은 공통 규칙·스킬 진입점
├── README.md
├── CONTRIBUTING.md
│
├── crates/
│   └── server/
│       ├── Cargo.toml
│       ├── migrations/       # PostgreSQL 스키마 변경의 정본
│       ├── src/
│       │   ├── main.rs
│       │   ├── lib.rs
│       │   ├── config.rs
│       │   ├── app.rs        # 구성·시작·종료 연결
│       │   ├── error.rs
│       │   ├── http/         # 라우팅·HTTP 공통 경계
│       │   ├── db/           # 풀·트랜잭션·RLS 문맥
│       │   ├── worker.rs     # 백그라운드 작업(O-19, 실행 역할로 켬)
│       │   ├── search.rs     # 통합 검색 색인·Meilisearch 연결(D-39)
│       │   ├── storage/      # 로컬·S3 구현
│       │   ├── search/       # Meilisearch 연결
│       │   └── features/
│       │       ├── accounts/
│       │       ├── workspaces/
│       │       ├── permissions/
│       │       ├── modules/  # 유형·모듈 인스턴스·참조 키
│       │       ├── documents/# 위키·모듈 문서·개인 노트
│       │       ├── collaboration/
│       │       ├── tasks/
│       │       ├── checklists/
│       │       ├── schedules/
│       │       ├── time_tracking/
│       │       ├── files/    # 자료·버전·업로드 세션
│       │       ├── references/
│       │       ├── notifications/
│       │       ├── audit/
│       │       ├── projects/
│       │       ├── research/
│       │       ├── study/
│       │       ├── courses/
│       │       └── hobbies/
│       └── tests/            # API·DB·권한 통합 테스트
│           ├── common/
│           └── fixtures/
│
├── apps/
│   └── web/
│       ├── package.json
│       ├── tsconfig.json
│       ├── vite.config.ts
│       ├── svelte.config.js
│       ├── src/
│       │   ├── app/          # 시작·화면 이동·전체 설정
│       │   ├── lib/
│       │   │   ├── api/      # 생성 client 연결 설정
│       │   │   ├── ui/       # 공통 UI
│       │   │   ├── editor/   # Tiptap·블록·Markdown
│       │   │   ├── viewers/ # PDF·Office·rhwp
│       │   │   └── features/
│       │   │       ├── workspaces/
│       │   │       ├── documents/
│       │   │       ├── tasks/
│       │   │       ├── checklists/
│       │   │       ├── schedules/
│       │   │       ├── time-tracking/
│       │   │       ├── files/
│       │   │       ├── research/
│       │   │       ├── study/
│       │   │       └── courses/
│       │   └── styles/
│       └── tests/
│           ├── component/
│           └── fixtures/
│
├── packages/
│   └── api-client/           # 계약에서 생성하는 TS client
│       ├── package.json
│       └── src/generated/
│
├── contracts/
│   └── openapi.json          # Rust 정의에서 생성
│
├── xtask/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── check.rs
│       ├── dev.rs
│       └── test.rs
│
├── tests/
│   └── e2e/                 # 브라우저를 통한 제품 흐름
│       ├── package.json
│       ├── playwright.config.ts
│       ├── specs/
│       └── fixtures/
│
├── deploy/
│   ├── docker/
│   │   ├── dev.Dockerfile
│   │   └── app.Dockerfile
│   ├── compose.dev.yaml
│   ├── compose.test.yaml
│   ├── compose.selfhost.yaml   # 셀프호스트(docs/self-hosting.md)
│   ├── selfhost.env.example
│   ├── postgres/initdb/       # 첫 설치 때 Keycloak DB(선택)
│   └── keycloak/              # 테스트 렐름, 셀프호스트 렐름 틀
│
├── docs/
│   ├── decisions.md
│   ├── product.md
│   ├── architecture.md
│   ├── development.md
│   ├── testing.md
│   ├── security.md
│   └── self-hosting.md
│
├── .agents/
│   └── skills/              # 실제로 필요한 역할별 스킬
└── .github/
    └── workflows/           # 공통 검사 명령 실행과 gate
```

## 검토자 메모 (제안, 미승인)

구조 기준 자체는 승인되었다. 아래는 초기화 시 D-47에 따라 검토자가 판단하고 이유를 기록할 사항이다.

- 결정 기록과 맞는 점: `contracts/openapi.json`을 Rust 정의에서 생성하고 `packages/api-client`를 그 계약에서 생성하는 구조는 O-22 권장안과 일치한다. 백그라운드 작업은 O-19 권장안대로 `crates/server/src/worker.rs`(단일 바이너리, 실행 역할, PostgreSQL 기록)로 두었다. `features/documents/`가 개인 노트까지 담는 것은 O-02(개인 노트 소유 구조)의 결과에 따라 분리될 수 있다.
- 프론트엔드 `features/`에 `projects`, `hobbies`, `notifications`, `audit`, `modules`가 없다. 서버 쪽에는 있으므로 의도적 생략인지 확인이 필요하다.
- `deploy/compose.test.yaml`에 Keycloak이 들어가야 한다(D-45, D-10). `compose.selfhost.yaml`의 Keycloak은 선택 프로필(O-25 해소).
- `docs/development.md`는 기존 `development-guidelines.md`를 이 이름으로 옮겼다. `product.md`, `architecture.md`, `testing.md`, `security.md`, `self-hosting.md`는 아직 없다.
- `AGENTS.md`와 `.agents/skills/`의 역할과 내용은 별도 지침이 필요하다.
