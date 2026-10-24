# 빌드·검사용 베이스 이미지 (ghcr.io/fvoci/groundzero-ci-base). 도구와 필요한 시스템 의존성만 담는다.
# 제품 소스·시크릿·실행 중 DB를 넣지 않는다. Docker 자체는 포함하지 않는다: CI 컨테이너 job은 서비스를
# job services로 받고(`cargo xtask test --external-services`), Compose 기반 통합 테스트는 호스트에서 `cargo xtask test`로 돈다.
# 제품 실행 이미지는 app.Dockerfile. 둘을 섞지 않는다.
FROM rust:1.99.1-bookworm@sha256:48b7f435b8fc6378ce43b44f15e3a47fd8919b2825f835c39bd4bd3da50af5b8

ARG NODE_MAJOR=24
ARG NODE_VERSION=24.22.1

# pnpm은 Corepack이 package.json의 `packageManager`(판·sha512 고정)대로 실행한다. 그 판과 pnpm 12가 처음 실행할 때 받는
# 플랫폼 바이너리를 이미지에 미리 받아 두고 모든 UID가 읽을 수 있게 한다. `--user`로 실행하면 이 캐시는 읽기 전용이므로
# 판이 바뀌면 이미지를 다시 만든다(base-image.yml이 package.json 변경에도 돈다).
ENV COREPACK_HOME=/opt/corepack \
    COREPACK_ENABLE_DOWNLOAD_PROMPT=0

RUN --mount=type=bind,source=package.json,target=/tmp/corepack/package.json \
    apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl gnupg git \
 && curl -fsSL https://deb.nodesource.com/setup_${NODE_MAJOR}.x | bash - \
 && apt-get install -y --no-install-recommends nodejs=${NODE_VERSION}-1nodesource1 \
 && corepack enable pnpm \
 && (cd /tmp/corepack && corepack install && pnpm --version) \
 && chmod -R a+rX "$COREPACK_HOME" \
 && rustup component add rustfmt clippy \
 && cargo install cargo-deny --version 0.20.2 --locked --jobs 4 \
 && rm -rf /var/lib/apt/lists/*

# UID/GID는 실행 시 `--user`로 호스트 사용자에 맞춘다 (1000:1000 고정 금지).
WORKDIR /work
