# 빌드·검사용 베이스 이미지 (ghcr.io/fvoci/groundzero-ci-base). 도구와 필요한 시스템 의존성만 담는다.
# 제품 소스·시크릿·실행 중 DB를 넣지 않는다. Docker 자체는 포함하지 않는다: CI 컨테이너 job은 서비스를
# job services로 받고(`cargo xtask test --external-services`), Compose 기반 통합 테스트는 호스트에서 `cargo xtask test`로 돈다.
# 제품 실행 이미지는 app.Dockerfile. 둘을 섞지 않는다.
FROM rust:1.99.0-bookworm

ARG NODE_MAJOR=24
ARG PNPM_VERSION=11.24.0

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl gnupg git \
 && curl -fsSL https://deb.nodesource.com/setup_${NODE_MAJOR}.x | bash - \
 && apt-get install -y --no-install-recommends nodejs \
 && npm install -g pnpm@${PNPM_VERSION} \
 && rustup component add rustfmt clippy \
 && cargo install cargo-deny --version 0.20.2 --locked \
 && rm -rf /var/lib/apt/lists/*

# UID/GID는 실행 시 `--user`로 호스트 사용자에 맞춘다 (1000:1000 고정 금지).
WORKDIR /work
