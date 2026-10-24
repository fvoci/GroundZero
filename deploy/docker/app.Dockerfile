# 제품 실행 이미지. 서버 바이너리와 정적 웹 자산만 담는다.
# 릴리스 태그의 이미지는 .github/workflows/release-image.yml이 빌드해 GHCR에 게시한다(D-87). 그 밖의 게시·배포는 하지 않는다.

FROM rust:1.99.1-bookworm@sha256:48b7f435b8fc6378ce43b44f15e3a47fd8919b2825f835c39bd4bd3da50af5b8 AS server-build
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY xtask ./xtask
# 제품 버전·소스 SHA(D-87). 릴리스 워크플로가 검증한 태그에서 정해 넘긴다(`0.0.1`, 전체 커밋 SHA). 둘 다 비우면 개발 빌드
# (`dev`)다. 하나만 넣거나 형식이 틀리면 컴파일이 멈춘다(crates/server/src/build_info.rs).
ARG FVOCI_RELEASE_VERSION=
ARG FVOCI_SOURCE_SHA=
RUN cargo build --release --locked --package fvoci-server

# HWP·HWPX 미리보기 WASM(D-76). 앱 도구 모음(위 server-build)과 따로, 고정한 rhwp 커밋·잠금 파일·기능·도구 판으로 만든다.
# 입력(deploy/rhwp)이 같으면 이 단계의 캐시를 그대로 쓴다. 판은 deploy/rhwp/inputs.env와 같아야 한다(build.sh가 확인).
FROM rust:1.93.1-bookworm@sha256:7c4ae649a84014c467d79319bbf17ce2632ae8b8be123ac2fb2ea5be46823f31 AS rhwp-build
RUN apt-get update && apt-get install -y --no-install-recommends jq brotli && rm -rf /var/lib/apt/lists/* \
 && rustup target add wasm32-unknown-unknown \
 && cargo install --locked wasm-bindgen-cli --version 0.2.127 \
 && cargo install --locked cargo-deny --version 0.20.2
COPY deploy/rhwp/build.sh deploy/rhwp/inputs.env deploy/rhwp/Cargo.lock deploy/rhwp/deny.toml /rhwp/
RUN sh /rhwp/build.sh

# `cargo xtask rhwp`가 이 단계만 꺼내 개발 서버와 고지 파일에 쓴다.
FROM scratch AS rhwp-out
COPY --from=rhwp-build /out /

FROM node:24.22.1-bookworm-slim@sha256:55fb5f5a527ea2c5c4a95f963e48ef90aa38c7b346054fdc8ee9957a2b55d205 AS web-build
# 해시 자원의 br 사본을 만드는 도구(아래 빌드 끝).
RUN apt-get update && apt-get install -y --no-install-recommends brotli && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml .npmrc tsconfig.base.json ./
# pnpm은 Corepack이 package.json의 `packageManager`(판·sha512 고정)대로 받아 실행한다.
ENV COREPACK_ENABLE_DOWNLOAD_PROMPT=0
RUN corepack enable pnpm && corepack install && pnpm --version
COPY apps ./apps
COPY packages ./packages
COPY contracts ./contracts
# HWP·HWPX 미리보기 WASM은 public/rhwp/<입력 해시>/로 들어가 웹 빌드와 함께 나간다(미리보기를 열 때만 불러옴).
COPY --from=rhwp-build /out/web ./apps/web/public/rhwp
# 해시 자원(서체·번들)에 br·gzip 사본을 더한다(D-86, `/rhwp`와 같은 방식). 원본은 그대로다.
RUN pnpm install --frozen-lockfile && pnpm --filter @fvoci/web run build \
 && find apps/web/dist/assets -type f -size +1k \( -name '*.js' -o -name '*.css' -o -name '*.svg' -o -name '*.otf' -o -name '*.ttf' \) \
      -exec gzip -9 -k -n {} + -exec brotli -q 11 -k {} +

FROM debian:bookworm-slim@sha256:7c7b2c966bc9ee8cedfeef67e0e279108992c77681fa595db4a9d65c06ccc587 AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --create-home fvoci \
 # 파일 저장소·백업 볼륨이 앱 사용자 소유로 만들어지도록 미리 둔다(이름 있는 볼륨은 처음에 이 소유를 따른다).
 && mkdir -p /data/files /backups && chown fvoci:fvoci /data/files /backups && chmod 0700 /data/files /backups
COPY --from=server-build /src/target/release/fvoci-server /usr/local/bin/fvoci-server
COPY --from=web-build /src/apps/web/dist /srv/fvoci/web
# 제3자 고지(D-60, D-68). `cargo xtask notices`가 서버 바이너리와 웹 번들에 든 패키지로 만들고 `cargo xtask check`가
# 최신 여부를 확인한다. 이미지 안 문서 위치와 웹(/THIRD_PARTY_NOTICES.txt) 두 곳에 둔다.
COPY THIRD_PARTY_NOTICES.txt /usr/share/doc/fvoci/THIRD_PARTY_NOTICES.txt
COPY THIRD_PARTY_NOTICES.txt /srv/fvoci/web/THIRD_PARTY_NOTICES.txt
USER fvoci
ENV FVOCI_BIND=0.0.0.0:8080 \
    FVOCI_WEB_DIR=/srv/fvoci/web \
    FVOCI_STORAGE_DIR=/data/files
EXPOSE 8080
HEALTHCHECK --interval=10s --timeout=5s --retries=6 CMD ["/usr/local/bin/fvoci-server", "healthcheck"]
ENTRYPOINT ["/usr/local/bin/fvoci-server"]
