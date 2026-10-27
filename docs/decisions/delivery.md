# 결정 기록 · 저장소·CI·배포·라이선스·도구

[결정 기록 색인](../decisions.md)의 한 주제다. 번호·앵커(`#d-xx`)는 색인 전체에서 고유하다. 아래 "현재 규칙 요약"은 이 주제의 유효한 규칙을 줄인 것이고, 정본은 그 아래 조항이다. 조항 안의 "개정 2026-10-25"는 그날 바뀐 문구이며 이전 문장은 조항의 원문 연결 또는 [개정 전 판](https://github.com/fvoci/GroundZero/blob/8349771265af71e70d628bd48abe94f815d8f29c/docs/decisions.md)에서 본다.

## 현재 규칙 요약

- Rust는 rustfmt·Clippy, TypeScript는 strict·타입 기반 ESLint이며 예외는 위치 한정이다(D-09). 도구는 rg·git grep·jq를 쓰고 셸 안 Python 헬퍼로 파일을 고치지 않는다. 저장소·CI에 래퍼를 더하지 않으며, 사용자 지시로 5950X에만 머신 설정으로 sccache를 넣었다. CI sccache(GHA 백엔드)는 #158 측정 뒤 사용자 확인으로만 넣는다(D-67).
- 개발·통합 시험은 격리된 Docker Compose이며 운영·개인 DB로 시험하지 않는다(D-10, D-63).
- CI는 공통 베이스 이미지 digest를 쓰고 YAML에 긴 셸을 두지 않는다. Supabase·browser job만 호스트 러너 예외다(D-64).
- 저장소·CI 이미지는 공개, 제품 이미지는 검증한 `vX.Y.Z` 태그에서만 게시하며 main 병합·태그·배포는 별도 승인이다(D-65, D-87).
- 컨테이너는 Debian 기반 digest 고정이다(D-85).
- 의존성 라이선스는 허용 목록(MIT·Apache-2.0·BSD·ISC·PostgreSQL·Zlib)이고 그 밖은 패키지·판 단위 예외로만 허용한다(D-60, D-68, D-72, D-73, D-84, D-86).
- 달리는 아비시니안 오버홀 전 계약 문서는 제거했고 새 계약·스킬은 그 오버홀의 4단계에서 쓴다(D-46, D-127).
- 개인 사용자는 내장 Keycloak·PostgreSQL·로컬 저장을 켜는 단일 Compose 프로필로 설치한다. 운영자 전제와 OIDC 전용은 그대로다(D-129).
- 변경 검토는 수정을 모은 최종 HEAD에서 한 번 한다. 작성 세션·통합 담당이 아닌 새 검수 세션이 `/code-review high`로 보고 CodeRabbit 결과와 함께 처분한다. CodeRabbit이 한도로 못 돌면 묶음별 검수 세션이 둘을 함께 대신한다. 인증·권한·RLS 변경은 병합 전 `/security-review`를 더한다. 오버홀과 그 밖의 코드 품질 작업에 모두 적용한다(D-130).
- 오버홀은 되풀이하며 날짜가 아니라 계획 때 사용자가 정한 별칭으로 부른다(지금 것은 "달리는 아비시니안"). D-126의 단계 틀을 기본으로 쓰고, SQL 기준 재통합은 정식 `v1.0.0` 전까지만, 수락 기준은 계획할 때 사용자에게 묻는다(D-131).

## 조항

<a id="d-09"></a>

- **D-09 Rust·TypeScript 품질 기준.** Rust는 rustfmt·Clippy를 필수 검사로 쓰고 Clippy 규칙은 선택하며, 고정 툴체인의 경고 오류 처리, 무분별한 unwrap·expect·panic과 오류 버림 금지, 자체 unsafe 기본 금지, 의미 있는 타입·async 책임을 지킨다. TypeScript는 strict·명시 옵션, svelte-check·타입 기반 ESLint, unknown에서 외부 입력 검증, any·이중 단언·@ts-ignore 금지, 비동기 오류·취소·오래된 응답 처리·수명주기 해제를 지킨다. Rust↔TS 계약·전송 표현·상태 정본을 단일화하고 런타임 검증을 유지한다. 세부·예외·복잡도 원칙과 이미 구현한 검사 설정은 [제거 전 개발 지침](https://github.com/fvoci/GroundZero/blob/8349771265af71e70d628bd48abe94f815d8f29c/docs/development.md)(D-127)과 O-22·O-23. 근거: 품질 기준 지침.

<a id="d-10"></a>

- **D-10 Docker Compose 개발·통합 테스트 환경.** PostgreSQL·Meilisearch·테스트 S3 저장소·격리된 Keycloak(D-45)을 컨테이너로 실행한다. 운영·개인 개발 DB로 시험하지 않는다. 버전·이미지·명령을 로컬과 CI에서 맞추고, 빌드/실행 이미지 분리·병렬 실행 격리·UID/GID 검증·시크릿 분리·자원별 동시 실행 제한·자기 자원만 정리를 지킨다. privileged·Docker 소켓·호스트 전체 마운트를 금지한다. 호스트 도구 허용 범위와 구현된 xtask·pnpm 명령은 [README의 명령표](../../README.md#명령)와 [제거 전 개발 지침](https://github.com/fvoci/GroundZero/blob/8349771265af71e70d628bd48abe94f815d8f29c/docs/development.md). 근거: Compose 지침.

<a id="d-46"></a>

- **D-46 저장소 초기 구조 기준.** [저장소 구조](https://github.com/fvoci/GroundZero/blob/8349771265af71e70d628bd48abe94f815d8f29c/docs/repo-structure.md)를 초기 기준으로 승인했다. 빈 폴더·파일을 미리 모두 만들지 않고 필요할 때 만든다. 개발 지침은 `development.md`로 통일해 옛 파일을 중복 유지하지 않았다. 당시 승인은 구조·문서 정리에 한정했고 제품 구현 착수와 구분했다. 두 문서는 D-127로 제거했고 달리는 아비시니안 오버홀 뒤 새로 쓴다. C-02 해소. 근거: 사용자(2026-10-17).

<a id="d-60"></a>

- **D-60 의존성 라이선스 정책.** 상업 이용·수정·재배포·SaaS에 적합한 허용적 라이선스를 우선한다. 기본 후보는 MIT·Apache-2.0·BSD-2-Clause·BSD-3-Clause·ISC·PostgreSQL License이며 실제 판·구성요소 조건을 확인한다. 직접/전이 의존성·복사 코드·WASM·폰트·아이콘 모두 대상이다. 카피레프트(GPL·AGPL·LGPL·MPL 등), SSPL·BUSL·비상업/목적 제한·상용 계약 필수·불명 라이선스는 조건·대안을 보고해 별도 승인받는다. 이중 라이선스의 선택 근거, 오픈소스 코어와 유료 확장, 앱 포함 코드/별도 서비스/이미지 OS 패키지를 구분한다. 저장소의 MIT 파일이나 Docker 분리만으로 전체 허용을 단정하지 않는다. 저작권·원문·NOTICE를 배포물에 보존하고 판을 고정해 변경 시 검사한다. 기존 Cargo·pnpm 도구를 쓰며 새 검사 프레임워크는 만들지 않는다. 선택한 뷰어 등의 조건 충돌도 임의 대체·예외 처리하지 않는다. 자체 공개 라이선스는 별도(O-27), 도구는 O-28, 추가 허용·예외는 D-68·D-72·D-74·D-84·D-86이다. 근거: 의존성 지침.

<a id="d-63"></a>

- **D-63 다음 흐름 수락 조건.** OIDC 로그인→공간 생성→문서 작성/저장/재열기에 무단 접근 거부·충돌 초안 보존·서버 재시작 후 재열기·실제 로그인/만료/로그아웃/권한 거부 시험(`--oidc`)을 포함한다. RLS는 관리자 계정이 아니라 실제 앱 역할로 시험한다. 근거: 사용자.

<a id="d-64"></a>

- **D-64 CI 구조 원칙.** 공통 `deploy/docker/dev.Dockerfile` 기반 이미지에 Rust·Node·pnpm·검사/시스템 도구를 고정하고 표준 xtask·pnpm 명령을 재사용한다. 이미지 변경·보안 갱신 때만 빌드하며 제품 커밋마다 만들지 않는다. 후속 job은 같은 digest를 쓰고 latest에 의존하지 않는다. YAML은 트리거·권한·의존·명령 호출·결과 업로드만 맡고 긴 설치/판정 셸은 두지 않는다. 이미지에 제품 소스·시크릿·실행 DB를 넣지 않고 서비스를 격리한다. 제품 빌드는 가능하면 한 번 만들어 SHA·아키텍처·옵션이 같은 것만 공유하고 캐시 없이도 검증돼야 한다. 병목 측정 전 샤딩·matrix·범용 프레임워크를 만들지 않는다. 이미지 게시 승인 전에는 로컬 빌드까지만 한다(D-65의 후속 승인, D-88의 Supabase job 및 아래 browser job 예외). 근거: CI 지침.
  - 구현 선택(2026-10-25 실제 브라우저 회귀 자동화 지시): `browser` job은 `cargo xtask browser`가 호스트 Docker의 격리 Compose·실제 서버·OIDC·Chromium 기동과 정리를 함께 맡으므로 호스트 러너에서 실행한다. Rust·Node·Corepack·pnpm·Playwright/Chromium 판은 고정하며, 기존 공통 이미지 job과 검사 항목·권한 계약은 유지한다. 실행·증거 수집 범위는 [제거 전 개발 지침](https://github.com/fvoci/GroundZero/blob/8349771265af71e70d628bd48abe94f815d8f29c/docs/development.md#ci)에 있다(D-127).

<a id="d-65"></a>

- **D-65 저장소·CI 이미지 공개.** 저장소는 공개 [fvoci/GroundZero](https://github.com/fvoci/GroundZero), CI 이미지는 공개 GHCR `ghcr.io/fvoci/groundzero-ci-base`다(AISFlow/fvoci·fvoci/FVOCI와 구분). D-07의 공개 조건은 제품 정식 공개이며 개발 저장소·CI 이미지는 먼저 공개한다. 승인 범위는 저장소/CI 이미지 공개 설정·초기화 브랜치 일반 push/PR·이미지 게시·PR CI/실패 수정이다. main 병합·force push·태그·제품 릴리스·SaaS 배포·유료 러너는 제외했다(제품 이미지의 후속 승인은 D-87). 무료 GitHub-hosted runner만 쓰고 게시 권한은 게시 job에만 주며 외부 PR 코드를 그 권한으로 실행하지 않는다. O-32 해소. 근거: 공개 승인 지침.

<a id="d-67"></a>

- **D-67 개발 도구 사용 원칙.** 검색은 경로·유형을 좁힌 rg, 목록은 `rg --files`, Git 추적 파일은 git grep, 간단한 파이프 필터는 grep이다. rg가 숨김·ignore 파일을 빼므로 결과 없음만으로 부재를 단정하지 않고 필요 시 해당 경로에만 옵션을 쓴다. JSON은 jq 등 구조화 도구, 변경은 문맥을 확인한 패치로 한다. 셸에서 Python을 생성하는 임시 파일 수정 헬퍼를 쓰지 않는다. 고정 버전의 설치된 도구를 PATH 짧은 명령으로 쓰고 절대 경로·래퍼·중복 설치를 더하지 않는다. 도구가 없으면 기존 도구로 가능한지 먼저 보며 정비를 큰 작업으로 키우지 않는다. 근거: 도구 지침; [AGENTS.md](../../AGENTS.md).
  - 개정 2026-10-27: 저장소·CI에는 래퍼를 더하지 않는다는 규칙은 그대로다. 예외로, 사용자가 직접 추가하라고 지시해(2026-10-27) 개발 머신 5950X에 빌드 캐시 sccache를 머신 설정(`~/.cargo/config.toml`의 `rustc-wrapper`)으로 추가했다. 다른 개발 머신에 넣을 때도 사용자 지시가 있어야 한다. 캐시는 그 머신의 로컬 디스크에 두고, 빌드·시험 속도를 재서 비교할 때는 sccache를 끄고(빈 `RUSTC_WRAPPER`·빈 target 디렉터리) 재며, sccache 효과는 켜고 데운 상태로 따로 적는다(#158). CI의 sccache GHA 백엔드는 #158의 측정 후보다. rust-cache만, sccache GHA, job 사이 빌드 공유를 같은 러너 종류에서 비교하고, 이득이 있을 때 사용자 확인을 받아서만 넣으며, `ACTIONS_RUNTIME_TOKEN`을 job 환경에 내보내는 설정은 `/security-review` 대상이다. 근거: 사용자 지시(2026-10-27, #158).

<a id="d-68"></a>

- **D-68 Zlib 라이선스 허용.** D-60 허용 목록에 Zlib을 추가하고 배포물에 `THIRD_PARTY_NOTICES.txt`를 넣는다. 당시 대상은 foldhash 0.1.5(sqlx 경로, 서버)·zlib-rs 0.6.7(rhwp 경로, 브라우저 WASM)·PDF 본문 검색에 도입할 zlib-rs(판·경로는 [제거 전 개발 지침 9절](https://github.com/fvoci/GroundZero/blob/8349771265af71e70d628bd48abe94f815d8f29c/docs/development.md)에 기록)다. 다른 라이선스를 함께 허용한 것이 아니며 O-30의 나머지는 D-72에서 별도 처리했다. 근거: 사용자(2026-10-17).

<a id="d-72"></a>

- **D-72 O-30 나머지 라이선스를 패키지·판 단위로 허용.** D-60·D-68의 일반 허용 목록은 그대로 두고 다음 범위만 예외로 둔다. 판·라이선스 식이 바뀌면 검사가 실패하고 재승인받는다. O-30 해소. 근거: 사용자(2026-10-17).
  - Cargo Unicode-3.0: icu_collections·icu_locale_core·icu_normalizer·icu_normalizer_data·icu_properties·icu_properties_data 2.3.0, icu_provider 2.3.1, litemap 0.8.3, potential_utf 0.1.6, tinystr 0.8.4, unicode-ident 1.0.27, writeable 0.6.4, yoke 0.8.3, yoke-derive 0.8.4, zerofrom·zerofrom-derive 0.1.8, zerotrie 0.2.5, zerovec 0.11.8, zerovec-derive 0.11.6. url→idna 경로의 서버 바이너리이며 `deny.toml`의 exceptions다.
  - npm 빌드·시험 도구: tslib 2.8.1(0BSD), lru-cache 11.5.3·minimatch 10.2.6(BlueOak-1.0.0), mdn-data 2.27.1(CC0-1.0), @csstools/color-helpers 6.1.2·@csstools/css-syntax-patches-for-csstree 1.1.15(MIT-0), lightningcss·플랫폼별 lightningcss-* 1.32.0·1.33.0(MPL-2.0), argparse 2.0.1(Python-2.0). `xtask/src/licenses.rs`의 NPM_EXCEPTIONS다.

<a id="d-73"></a>

- **D-73 PDF 본문 검색과 RUSTSEC-2026-0192 단일 예외.** pdf-extract 0.12.1→lopdf 0.42.0→ttf-parser 0.25.1의 유지 중단 알림 하나만 cargo-deny advisories에서 무시한다(취약점 아님, 수정 판 없음, 글자 추출 경로에서는 미사용). 사유를 기록하고 lopdf가 ttf-parser를 제거하거나 유지되는 판으로 바꾸면 예외를 지운다. 다른 보안 검사는 유지한다. 근거: 사용자(2026-10-17).

<a id="d-84"></a>

- **D-84 Paraglide 빌드 도구의 라이선스 예외.** `Apache-2.0 WITH LLVM-exception`을 다음 판의 **빌드 시점만** 허용한다: @bytecodealliance/jco-transpile 0.13.0, @bytecodealliance/jco-node-fs 0.3.2와 그 -darwin-arm64/-linux-x64-gnu/-linux-arm64-gnu 0.3.2, @bytecodealliance/preview2-shim 0.24.1·0.25.0, @bytecodealliance/preview3-shim 0.6.1. Paraglide 메시지 컴파일 경로이며 제품 이미지·웹 번들 포함은 허용하지 않는다. 판 변경은 재검토, 일반 허용 목록은 유지한다. 정확한 예외는 `xtask/src/licenses.rs`, [의존 경로·당시 확인](https://github.com/fvoci/GroundZero/blob/abd545a250ff1fd9cc166b3c078c4dff76846724/docs/decisions.md#L172)은 원문을 따른다. 근거: 사용자(2026-10-17, “Approve, build-time only”).

<a id="d-85"></a>

- **D-85 컨테이너 이미지는 Debian 기반·digest 고정.** 제품 빌드/실행과 PostgreSQL은 Debian 12(bookworm) 기반이다. 공식 Debian 판이 없는 Meilisearch·SeaweedFS(Alpine), Keycloak(Red Hat UBI)은 공식 이미지를 쓴다. 모든 제3자 이미지(Compose/CI/Dockerfile FROM)는 버전 태그+다중 아키텍처 index digest로 고정한다. PostgreSQL의 지원 메이저는 D-100, 실제 고정 판은 배포 파일을 따른다. Alpine→Debian은 uid 70→999와 musl→glibc 정렬 차이 때문에 기존 볼륨을 그대로 연결하지 않고 백업→새 설치→복구한다. 근거: 사용자(2026-10-18).

<a id="d-86"></a>

- **D-86 Noto Sans KR·Noto Serif KR·D2Coding 서체.** Noto Sans KR 2.004·Noto Serif KR 2.003·D2Coding 1.4.0의 Regular/Bold 여섯 파일에만 OFL-1.1 예외를 허용한다. 다른 파일·라이선스 전체 허용이 아니다. 외부 CDN·글자별 조각 요청은 금지한다. 근거: 사용자(2026-10-18), O-37 해소.
  - 원본 파일을 변환/부분집합화하지 않고 assets/fonts에 둔다. notices 검사는 여섯 파일+라이선스 둘(숨김 제외), O-37의 승인 SHA-256 일치, 실제 번들 서체 제한을 확인한다. 출처·판·해시·저작권·상표·원문은 THIRD_PARTY_NOTICES에 보존한다. 파일/해시·라이선스 세부는 O-37의 고정 원문을 따른다.
  - preload 없이 font-display: swap을 쓴다. 본문/UI는 첫 화면에서 Sans Regular/Bold를 받는다. 코드·원문·Markdown/LaTeX/CSV·리비전 비교가 있는 화면에서만 D2Coding을 받는다. 번호/단축키 등 짧은 UI 고정폭은 시스템 서체다. Serif는 이 브라우저의 ‘문서 본문: 명조’ 선택 후 문서를 열 때만 받으며 계정/API/문서 형식을 바꾸지 않는다.
  - 해시 자원에 br·gzip 사본을 만들고 /assets에서 전송하되 원본은 유지한다. KR 파일에 없는 일본어 한자 등은 시스템 일본어 서체로 대체한다. [실제 크기·글자 범위 확인](https://github.com/fvoci/GroundZero/blob/abd545a250ff1fd9cc166b3c078c4dff76846724/docs/decisions.md#L177)은 당시 측정이다.

<a id="d-87"></a>

- **D-87 제품 이미지 릴리스와 버전 표시.** 검증한 원격 `vX.Y.Z` 태그(main에 포함된 커밋)의 제품 이미지를 Actions에서 GHCR `ghcr.io/fvoci/groundzero:X.Y.Z`로 게시한다. CI 베이스 이미지와 구분하고 latest는 없다. D-65 중 제품 이미지 게시만 추가 승인했으며 main 병합·태그 생성/이동·SaaS 배포는 별도다. 근거: 사용자(2026-10-20).
  - 버전은 X.Y.Z, 소스는 전체 태그 SHA이며 CI가 한 번 정해 빌드 인자·OCI 라벨·확인값에 같이 쓴다. Cargo 0.1.0은 제품 버전이 아니다. 서버 healthz의 build/시작 로그/version 명령과 계정 메뉴에 같은 값을 낸다. 미주입 빌드는 dev, 한 값만 있거나 형식 오류면 컴파일 실패다.
  - 주입 전 v0.0.1만 태그를 옮기지 않고 예외 빌드한다(라벨에만 버전/SHA, 앱에는 없음). 다른 주입 전 태그는 거부한다.
  - 무료 GitHub-hosted 네이티브 amd64·arm64에서 각각 빌드/확인한 뒤 게시한다. 같은 이미지 태그는 SHA가 같을 때만 유지하고 다르면 실패한다. 게시 job만 기존 GITHUB_TOKEN의 게시 권한을 가지며 PR은 게시하지 않는다.

<a id="d-127"></a>

- **D-127 계약 문서 제거.** 오버홀 전 개발 지침(`docs/development.md`)·SQL UUID 계약·저장소 구조·마이그레이션 안내·`.agents/skills`·`CONTRIBUTING.md`를 제거했다. 오버홀 동안 기준은 이 문서와 코드·시험이고, 새 계약 문서와 스킬은 오버홀 단계가 끝날 때 그 코드를 보고 쓴다. 제거 전 판은 [8349771](https://github.com/fvoci/GroundZero/blob/8349771265af71e70d628bd48abe94f815d8f29c/docs/development.md)에서 본다. 소스 주석의 옛 앵커는 오버홀 때 함께 고친다. 근거: 사용자(2026-10-25).
  - 개정 2026-10-27: 이 조항의 "오버홀"은 달리는 아비시니안 오버홀이고, 새 계약 문서와 스킬은 그 4단계에서 쓴다(D-131). 근거: 사용자(2026-10-27).

<a id="d-129"></a>

- **D-129 개인용 단일 Compose 프로필.** 운영자가 OIDC·DB·백업을 준비한다는 전제(D-45·D-51·D-100)는 유지한다. 대신 셀프호스트 Compose에 내장 Keycloak·PostgreSQL·로컬 파일 저장을 켜는 프로필 하나를 두어 개인 사용자가 명령 한 번으로 띄우게 한다. 새 서비스·자체 비밀번호 로그인은 넣지 않는다(X-09, O-25의 선택 프로필 안). 백업은 CLI 안내만 둔다. 달리는 아비시니안 오버홀 4단계(D-126)에서 한다. O-39 해소. 근거: 사용자(2026-10-25).

<a id="d-130"></a>

- **D-130 변경 검토 방식.** 오버홀(D-131)과 그 밖의 코드 품질 작업의 PR에 공통으로 적용하며, 달리는 아비시니안 오버홀에서는 D-128의 2026-10-27 개정(R5 B·#156)의 검토 규칙을 대체한다. 용어: 작성 세션은 코드를 쓴 에이전트 세션, 통합 담당은 작성 결과를 PR 브랜치에 합치는 세션(오버홀에서는 그 단계의 작업 배정과 통합을 맡은 코디네이터 세션, 그 밖에는 PR을 올린 세션), 검수 세션은 통합 담당이 PR 브랜치에서 새 작업 트리(Orca를 쓰면 PR 브랜치 워크트리 아래에 만든 워크트리)를 만들어 띄운 새 Claude Code 세션이다. (1) 시점: 수정 중에는 해당 단위·컴포넌트 시험과 로컬 `cargo xtask check`로 확인하고 커밋마다 차분 검토를 배정하거나 push해 CI를 다시 돌리지 않는다. 수정을 모은 최종 HEAD에서 한 번 검토하고 필수 CI 전부 SUCCESS를 확인한다. 검토로 고친 것이 있으면 그 차분을 새 검수 세션이 다시 검토하고, 수정된 최신 HEAD에서 필수 CI 전부 SUCCESS를 다시 확인해 그 SHA와 결과를 PR 본문의 시험 결과 표에 적는다. (2) 작성·검수 분리: 작성 세션과 통합 담당은 최종 검수를 하지 않는다(통합 담당은 합친 결과가 맞는지까지만 본다). 최종 검토는 CodeRabbit 결과의 처분과 함께, 검수 세션이 최종 HEAD에서 Claude Code `/code-review high`를 돌린 결과로 한다. CodeRabbit이 검토 한도(변경 파일 300개 초과 등)로 돌지 못하면 변경 파일을 한도 안의 묶음으로 나눠 묶음마다 검수 세션을 하나씩 만들고, 그 검토가 CodeRabbit과 `/code-review`를 함께 대신한다. 이때 각 검수 세션은 근거표와 모든 지적의 처분을 남기고, CodeRabbit은 PASS가 아니라 NOT RUN(한도 초과)으로 적으며, PR·base·필터·권한·비용은 바꾸지 않는다. 작성 쪽과 다른 모델의 눈을 더하기 위해, 쓸 수 있으면 Codex CLI(`gpt-6-astra`, 읽기 전용)로 같은 범위를 한 번 더 보고, 쓸 수 없으면(Codex 한도 등) 그 사실을 PR 본문에 적는다. 강도 max·ultra는 사용자가 지정할 때만 쓴다. (3) 인증·권한·RLS를 바꾸는 PR은 병합 전에 통합 담당이 띄운 새 세션이 Claude Code `/security-review`를 돌린다. 달리는 아비시니안 오버홀에서 이미 병합된 1·2단계(#139·#140)의 권한 모델은 4단계 코디네이터가 4단계 PR 병합 전 최종 HEAD에서 새 세션으로 범위 `db2f411c`(1단계 병합 직전 main)..그 HEAD를 지정해 한 번 점검한다. (4) 코드를 고치는 기능(Claude Code `/simplify`, `/code-review --fix`)은 병합 게이트 중에는 쓰지 않고, 쓰면 그 결과도 새 검수 세션이 다시 검토한다. (5) 끝난 작성·검수 세션(오버홀에서는 워커)과 그 작업 트리는 결과를 통합한 즉시 정리한다. 미커밋 변경이 있거나 커밋이 PR 브랜치·origin에 없으면 지우지 않고 이유를 남긴다. 근거: 사용자(2026-10-27, #156).

<a id="d-131"></a>

- **D-131 오버홀 진행 틀.** 오버홀은 코드 품질 차원에서 되풀이하며 한 달에도 여러 번 할 수 있다. 그래서 날짜로 부르지 않고, 오버홀을 계획할 때 사용자가 정한 별칭(예: "뜨거운 난로")으로 부른다. D-111~D-129와 D-126을 계획으로 한 지금 오버홀(#139~)의 별칭은 "달리는 아비시니안"이다. (1) 기본 틀은 D-126과 같다: SQL → 서버 → 웹 → 도구·CI 단계별 Draft PR, 앞 단계부터 이어 병합, 관련 열린 이슈는 해당 단계가 흡수. 단계 범위와 흡수 이슈는 오버홀마다 사용자가 정한다. (2) 정식 `v1.0.0` 원격 태그(D-92의 경계) 전의 오버홀은 D-107대로 SQL 기준 파일을 다시 통합하며, 새 마이그레이션 파일을 더하지 않고 기준 파일을 고친다. 그 뒤의 오버홀은 기존 설치가 올라올 수 있게 새 마이그레이션을 더한다. (3) 수락 기준은 오버홀을 계획할 때 사용자에게 묻는다. D-128은 달리는 아비시니안 오버홀의 수락 기준이다. (4) 검토 방식은 D-130을 따른다. 근거: 사용자(2026-10-27).
