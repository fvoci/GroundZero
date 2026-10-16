# 제품 실행 이미지. 서버 바이너리와 정적 웹 자산만 담는다.
# 이번 초기화에서는 작성만 하고 빌드·배포는 검증하지 않았다 (승인 범위 밖).

FROM rust:1.99.0-bookworm AS server-build
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY xtask ./xtask
RUN cargo build --release --locked --package fvoci-server

FROM node:24-bookworm-slim AS web-build
ARG PNPM_VERSION=11.24.0
RUN npm install -g pnpm@${PNPM_VERSION}
WORKDIR /src
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml .npmrc tsconfig.base.json ./
COPY apps ./apps
COPY packages ./packages
COPY contracts ./contracts
RUN pnpm install --frozen-lockfile && pnpm --filter @fvoci/web run build

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --create-home fvoci
COPY --from=server-build /src/target/release/fvoci-server /usr/local/bin/fvoci-server
COPY --from=web-build /src/apps/web/dist /srv/fvoci/web
# 라이선스 고지는 라이선스 검사 통과 후 NOTICE 생성 단계를 추가한다 (D-60).
USER fvoci
ENV FVOCI_BIND=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/fvoci-server"]
