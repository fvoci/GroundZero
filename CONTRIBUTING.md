# 기여 절차

1. 변경 전에 [docs/decisions.md](docs/decisions.md)에서 관련 확정 결정과 열린 항목을 확인한다. 기능 범위·권한·삭제·보존·비용·외부 서비스에 영향이 있으면 구현 전에 사용자 결정으로 올린다.
2. 품질 기준은 [docs/development.md](docs/development.md)를 따른다. 예외는 최소 범위에 이유를 적는다.
3. 제출 전 `cargo xtask check`와 `cargo xtask test`를 통과시킨다. 검사를 끄거나 건너뛰어 통과시키지 않는다.
4. API를 바꾸면 `cargo xtask openapi`와 `pnpm run generate:api`로 계약과 클라이언트를 함께 갱신한다.
5. 의존성을 추가·변경하면 `cargo xtask licenses`를 돌리고, 허용 목록 밖의 결과는 예외를 넣지 말고 보고한다.
6. 작업 보고에는 실행 명령·대상 커밋·결과(PASS/FAIL/NOT RUN)·남은 제한을 적는다.
