# fvoci 공통 규칙

짧고 도구 중립적인 진입점이다. 세부는 링크된 문서와 스킬에 있다.

## 기준 문서

- 결정 기록: `docs/decisions.md` (확정 결정 D, 미승인 제안 P, 열린 항목 O, 제외 X, 보류 H)
- 개발 절차·품질 기준: `docs/development.md`
- 저장소 배치: `docs/repo-structure.md`

## 빌드·검사 명령

- `pnpm install --frozen-lockfile`
- `cargo xtask check` — 포맷·lint·타입·단위 테스트·라이선스·계약
- `cargo xtask test` — 격리된 Compose로 통합 테스트 (`--oidc`로 Keycloak 포함)
- `cargo xtask dev`, `pnpm --filter @fvoci/web dev`, `cargo xtask down`
- `cargo xtask openapi` 후 `pnpm run generate:api`

## 변경 종류별 읽을 스킬

- Rust 서버·API·DB·권한 변경: `.agents/skills/rust-api-db/SKILL.md`
- Svelte·TypeScript·UI 변경: `.agents/skills/svelte-ui/SKILL.md`
- 테스트·Docker·CI 변경: `.agents/skills/test-docker-ci/SKILL.md`

관련 없는 스킬은 읽지 않는다.

## 시크릿·권한·데이터 보호

- 시크릿을 코드·이미지·Git·로그에 넣지 않는다. 테스트 자격 증명은 `deploy/` 안의 명백한 테스트 값만 쓴다.
- 권한은 서버에서 검사한다. 읽기와 수정·복제·다운로드 권한을 구분한다. 개인 노트와 개인 시간 기록은 워크스페이스 관리자에게도 비공개다.
- 운영 서비스나 개인 개발 DB를 테스트에 쓰지 않는다.

## 범위 확대와 사용자 확인이 필요한 조건

- 기능 범위 축소·확대, 권한 확대, 삭제·보존 정책, 비용, 새 외부 서비스 추가
- 허용 목록 밖 라이선스의 의존성 도입
- 검사 규칙 완화 (파일·프로젝트 전체 해제 금지)
- 보류 항목(H)과 범위 제외(X)에 해당하는 구현

그 밖의 구현 선택은 승인된 원칙 안에서 판단하고 이유를 짧게 기록한다.
