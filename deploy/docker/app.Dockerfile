# 제품 실행 이미지. 서버 바이너리와 정적 웹 자산만 담는다.
# 로컬 빌드·실행은 검증한다. 레지스트리 push·공개 배포는 별도 승인 대상이다.

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
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --create-home fvoci \
 # 파일 저장소·백업 볼륨이 앱 사용자 소유로 만들어지도록 미리 둔다(이름 있는 볼륨은 처음에 이 소유를 따른다).
 && mkdir -p /data/files /backups && chown fvoci:fvoci /data/files /backups && chmod 0700 /data/files /backups
COPY --from=server-build /src/target/release/fvoci-server /usr/local/bin/fvoci-server
COPY --from=web-build /src/apps/web/dist /srv/fvoci/web
# 라이선스 고지는 라이선스 검사 통과 후 NOTICE 생성 단계를 추가한다 (D-60).
USER fvoci
ENV FVOCI_BIND=0.0.0.0:8080 \
    FVOCI_WEB_DIR=/srv/fvoci/web \
    FVOCI_STORAGE_DIR=/data/files
EXPOSE 8080
HEALTHCHECK --interval=10s --timeout=5s --retries=6 CMD ["/usr/local/bin/fvoci-server", "healthcheck"]
ENTRYPOINT ["/usr/local/bin/fvoci-server"]
