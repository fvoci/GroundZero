# 여러 워크트리에서 나란히 작업하기

여러 작업을 Git 워크트리로 나눠 한 컴퓨터에서 함께 돌리는 방법이다. 손으로 `git worktree`를 써도 되고, 워크트리 기반 도구(예: Orca)를 써도 같다. 규칙의 근거는 결정 기록 D-10(격리된 Compose, 자기 자원만 정리)과 D-67(도구·래퍼)이다.

## 1. 위계와 정리 기준

- 작업마다 워크트리 하나를 쓴다. 여러 작업을 한 브랜치로 모을 때는 통합 브랜치의 워크트리를 부모로 두고, 작업 워크트리는 그 통합 브랜치에서 갈라 만든다.

  ```sh
  git worktree add -b <작업 브랜치> ../<작업 이름> <통합 브랜치>
  ```

- 작업 결과를 통합 브랜치에 합치고 확인했으면 그 워크트리를 지운다. 다음 중 하나라도 있으면 지우지 않고 이유를 남긴다.
  - 커밋하지 않은 변경(`git status --porcelain`이 비어 있지 않다)
  - 통합 브랜치에도 원격에도 없는 커밋(`git merge-base --is-ancestor <커밋> <통합 브랜치>`가 실패하고 `git branch -r --contains <커밋>`도 비어 있다)
- 지울 때는 `git worktree remove <경로>`를 쓴다. `--force`로 남은 변경을 버리지 않는다.

## 2. 새 워크트리 데우기

새 워크트리의 `target/`은 비어 있어 처음 빌드가 오래 걸린다. 부모 워크트리의 `target/`을 복사해 시작한다. 경로가 다른 워크트리 사이에서는 sccache 같은 컴파일 캐시가 거의 적중하지 않는다(#158 측정). 그래서 캐시보다 복사가 낫다.

```sh
# Linux. 파일 시스템이 지원하면 reflink로 복제해 공간과 시간이 거의 들지 않는다.
cp -a --reflink=auto ../<부모>/target ./target
# macOS(APFS). -c는 clonefile 복제다.
cp -cR ../<부모>/target ./target
```

복사본에는 부모의 실행 산출물도 들어 있다.

- `target/<종류>/run.lock`: 잠금은 파일이 아니라 열린 파일의 잠금이라, 남은 파일은 새 실행을 막지 않는다.
- `target/dev-files`: 부모 개발 환경의 첨부다. 이 워크트리의 개발 DB와 맞지 않으니 필요 없으면 지운다.
- `target/browser/runs/`: 부모의 브라우저 시험 원시 출력이다. 세션 쿠키 같은 값이 남을 수 있어 공개하지 않는다.

## 3. 동시 빌드 수와 임시 파일

- 무거운 빌드·시험은 한 컴퓨터에서 동시에 몇 개만 돌린다. `cargo xtask check`·`test`·`browser`, release 빌드, Playwright가 여기에 든다. 몇 개까지 돌릴지는 그 컴퓨터의 코어·메모리에 맞춰 정한다.
- 함께 돌리면 `CARGO_BUILD_JOBS`로 코어를 나눈다. 예를 들어 둘이면 각자 코어 수의 절반을 쓴다. CI의 browser job도 같은 방식으로 값을 정해 둔다(`.github/workflows/ci.yml`의 `CARGO_BUILD_JOBS`).
- `/tmp`가 메모리(tmpfs)인 컴퓨터에서는 큰 임시 파일이 메모리를 차지한다. 무거운 명령은 디스크 쪽 디렉터리를 `TMPDIR`로 주고 돌린다.

  ```sh
  TMPDIR=<디스크의 임시 디렉터리> CARGO_BUILD_JOBS=<나눈 값> cargo xtask test
  ```

## 4. Docker 캐시와 정리 범위

- 네트워크가 느리면 시험 이미지와 BuildKit 캐시를 다시 받는 데 오래 걸린다. `docker system prune`·`docker image prune -a`·`docker builder prune`처럼 넓게 지우는 명령을 쓰지 않는다. 이런 명령은 다른 워크트리의 컨테이너·이미지·캐시도 지운다.
- 정리는 그 실행이 만든 자원만 한다. 저장소 도구가 이렇게 정리한다.
  - `cargo xtask test`·`cargo xtask browser`: 끝나면 자기 Compose 프로젝트만 내린다. `--keep`이면 남긴다.
  - 강제로 끝났으면(SIGKILL 등) 시작할 때 출력한 복구 명령으로 그 프로젝트만 멈춘다. 이 명령은 볼륨을 지우지 않는다.
  - release 이미지 smoke: 실행 라벨이 붙은 자원만 지운다(`.github/workflows/release-image.yml`의 `SMOKE_RECOVER`).
- `cargo xtask down`은 이 체크아웃의 개발·시험·브라우저·Supabase 시험 프로젝트를 볼륨까지 지운다. 개발 DB 내용도 사라진다. `cargo xtask test --supabase --keep`으로 남긴 시험 DB도 사라진다. 다른 체크아웃은 건드리지 않는다.

## 5. 워크트리별 시험 격리

워크트리마다 시험이 서로 겹치지 않는 까닭은 다음과 같다.

- Compose 프로젝트 이름: 체크아웃 경로의 해시와 종류(dev·test·browser·supabase)로 만든다. 같은 이름의 디렉터리라도 경로가 다르면 다른 프로젝트다.
- 호스트 포트: 시험 서비스는 `127.0.0.1::<포트>`로 Docker가 빈 포트를 고르고, xtask가 `docker compose port`로 찾는다.
- 시험 DB: tmpfs라 컨테이너가 멈추면 내용이 사라진다.
- 실행 잠금: 같은 체크아웃에서 같은 종류를 두 번 돌리면 나중 실행이 무엇도 건드리기 전에 실패한다(`target/<종류>/run.lock`). `test`와 `browser`는 프로젝트와 잠금이 따로라 함께 돌 수 있다.
- 고정 포트 예외: 개발 Keycloak은 로그인 사용자 식별(발급자 + subject)이 바뀌지 않게 `127.0.0.1:8180`에 고정한다. 두 워크트리에서 `cargo xtask dev --oidc`를 함께 띄우려면 한쪽에 `FVOCI_DEV_KEYCLOAK_PORT`로 다른 포트를 준다.
