# 빌드·검사용 이미지. CI와 로컬 최종 검증이 같은 도구 버전을 쓰도록 고정한다.
# 제품 실행 이미지는 app.Dockerfile. 둘을 섞지 않는다.
FROM rust:1.99.0-bookworm

ARG NODE_MAJOR=24
ARG PNPM_VERSION=11.24.0

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl gnupg docker.io docker-compose-v2 \
 && curl -fsSL https://deb.nodesource.com/setup_${NODE_MAJOR}.x | bash - \
 && apt-get install -y --no-install-recommends nodejs \
 && npm install -g pnpm@${PNPM_VERSION} \
 && rustup component add rustfmt clippy \
 && cargo install cargo-deny --version 0.20.2 --locked \
 && rm -rf /var/lib/apt/lists/*

# UID/GID는 실행 시 `--user`로 호스트 사용자에 맞춘다 (1000:1000 고정 금지).
WORKDIR /work
