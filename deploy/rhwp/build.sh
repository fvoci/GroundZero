#!/bin/sh
# HWP·HWPX 미리보기용 rhwp WASM 빌드(D-76). deploy/docker/app.Dockerfile의 rhwp-build 단계 안에서 돈다.
# 고정한 상위 커밋을 필요한 경로만 받아, 우리가 고정한 잠금 파일로 `pdf-import` 없이 wasm32 라이브러리를 만들고
# wasm-bindgen(web)으로 묶는다. 실제로 브라우저에 들어가는 그래프를 cargo-deny로 검사하고(deny.toml), 고지 자료와
# 크기 보고를 만든다. 결과: /out/web/<입력 해시>/(rhwp.js·rhwp_bg.wasm·.gz·.br), /out/web/manifest.json,
# /out/notices/(크레이트별 라이선스 원문과 목록), /out/report.json.
set -eu
here=/rhwp
. "$here/inputs.env"

# 도구 판이 입력과 같은지(다르면 같은 결과라고 할 수 없다).
rustc --version | grep -q "^rustc $RUST_VERSION " || { echo "rustc must be $RUST_VERSION: $(rustc --version)"; exit 1; }
wasm-bindgen --version | grep -q " $WASM_BINDGEN_VERSION\$" || { echo "wasm-bindgen must be $WASM_BINDGEN_VERSION"; exit 1; }
cargo deny --version | grep -q " $CARGO_DENY_VERSION\$" || { echo "cargo-deny must be $CARGO_DENY_VERSION"; exit 1; }
hash=$(cat "$here/build.sh" "$here/inputs.env" "$here/Cargo.lock" "$here/deny.toml" | sha256sum | cut -c1-16)

# 소스: 고정 커밋, 빌드에 필요한 경로만(작업 공간 구성원의 매니페스트·소스 포함).
mkdir -p /src && cd /src
git init -q .
git remote add origin "$RHWP_REPO"
git fetch -q --depth 1 --filter=blob:none origin "$RHWP_COMMIT"
git sparse-checkout set --no-cone /Cargo.toml /Cargo.lock /build.rs /rust-toolchain.toml /LICENSE \
  /THIRD_PARTY_LICENSES.md /src/ /crates/ /vendor/ /saved/blank2010.hwp /tools/rhwp-subsecond/ \
  /bindings/Native/Cargo.toml /bindings/Native/src/ /tools/batch-convert/Cargo.toml /tools/batch-convert/src/ \
  /tools/llm_verifier/verdict_protocol/Cargo.toml /tools/llm_verifier/verdict_protocol/src/ \
  /tools/llm_verifier/claim_bind/Cargo.toml /tools/llm_verifier/claim_bind/src/ \
  /tools/llm_verifier/criteria_decomp/Cargo.toml /tools/llm_verifier/criteria_decomp/src/
git checkout -q FETCH_HEAD
test "$(git rev-parse HEAD)" = "$RHWP_COMMIT"
# 일부만 받은 저장소는 cargo가 파일 목록을 git으로 읽다 실패하므로, 커밋을 확인한 뒤 git 정보를 지운다.
rm -rf .git
# 상위의 rust-toolchain.toml이 이 폴더에서 도구 모음을 고르므로 여기서도 판을 확인한다.
rustc --version | grep -q "^rustc $RUST_VERSION " || { echo "rustc in /src must be $RUST_VERSION: $(rustc --version)"; exit 1; }
cp "$here/Cargo.lock" Cargo.lock

# 감사: 상위 작업 공간은 다른 구성원이 기본 기능(pdf-import)을 켜 cargo-deny가 기능을 합치므로, 구성원을 crates/*로
# 줄인 사본에서 실제 브라우저 그래프를 검사한다. 사본의 잠금 파일은 패키지가 빠지기만 하고 판은 바뀌지 않아야 한다.
mkdir -p /audit
cp -R Cargo.toml build.rs src crates vendor saved /audit/
cp Cargo.lock /audit/Cargo.lock
sed -i 's/^members = \[.*\]$/members = ["crates\/*"]/' /audit/Cargo.toml
grep -q '^members = \["crates/\*"\]$' /audit/Cargo.toml
(cd /audit && cargo metadata --format-version 1 >/dev/null)
pkgs() { grep -A1 '^name = ' "$1" | grep -v '^--' | paste - - | sort; }
added=$(pkgs Cargo.lock > /tmp/a; pkgs /audit/Cargo.lock > /tmp/b; comm -13 /tmp/a /tmp/b)
test -z "$added" || { echo "audit lock changed versions: $added"; exit 1; }
(cd /audit && cargo deny --locked --config "$here/deny.toml" check advisories bans licenses sources)
# 같은 입력으로 다시 빌드하지 않을 때도 새 권고를 다시 볼 수 있게 감사 작업 공간을 결과에 남긴다(cargo xtask rhwp --audit).
mkdir -p /out/audit
(cd /audit && cp -R Cargo.toml Cargo.lock build.rs src crates vendor saved /out/audit/)

# 빌드(브라우저용 라이브러리만, pdf-import 없음).
cargo build --locked --lib --release --target wasm32-unknown-unknown --no-default-features --features "$FEATURES"
out=/out/web/$hash
mkdir -p "$out"
wasm-bindgen --target web --out-dir "$out" target/wasm32-unknown-unknown/release/rhwp.wasm
rm -f "$out"/*.d.ts "$out"/package.json
gzip -9 -k -n "$out/rhwp_bg.wasm" "$out/rhwp.js"
brotli -q 11 -k "$out/rhwp_bg.wasm" "$out/rhwp.js"
size() { wc -c < "$1" | tr -d ' '; }
printf '{"dir":"%s","commit":"%s","features":"%s"}\n' "$hash" "$RHWP_COMMIT" "$FEATURES" > /out/web/manifest.json

# 고지 자료: 브라우저에 들어가는 크레이트(proc-macro·빌드 의존 제외)마다 라이선스 식·원문.
cargo tree --locked -p rhwp --target wasm32-unknown-unknown -e normal,no-proc-macro --no-default-features \
  --features "$FEATURES" --prefix none | sed 's/ (\*)$//; s/ (proc-macro)$//' | sort -u > /tmp/tree.txt
cargo metadata --locked --format-version 1 --filter-platform wasm32-unknown-unknown > /tmp/meta.json
mkdir -p /out/notices
: > /out/notices/index.tsv
while read -r name ver rest; do
  ver=${ver#v}
  jq -r --arg n "$name" --arg v "$ver" \
    '.packages[] | select(.name==$n and .version==$v) | [.name, .version, (.license // "UNKNOWN"), (.repository // ""), .manifest_path] | @tsv' \
    /tmp/meta.json | head -n 1 > /tmp/row
  test -s /tmp/row || { echo "no metadata for $name $ver"; exit 1; }
  cat /tmp/row >> /out/notices/index.tsv
  dir=$(dirname "$(cut -f5 /tmp/row)")
  # 이 저장소 안의 크레이트(rhwp 본체·crates/*)는 저장소 루트의 LICENSE를 쓴다.
  case "$dir" in /src*) test -f "$dir/LICENSE" || dir=/src ;; esac
  mkdir -p "/out/notices/$name-$ver"
  for f in "$dir"/LICENSE* "$dir"/LICENCE* "$dir"/COPYING* "$dir"/NOTICE* "$dir"/COPYRIGHT*; do
    test -f "$f" && cp "$f" "/out/notices/$name-$ver/"
  done
done < /tmp/tree.txt
cp /src/THIRD_PARTY_LICENSES.md /out/notices/rhwp-THIRD_PARTY_LICENSES.md

printf '{"commit":"%s","dir":"%s","crates":%s,"wasm_bytes":%s,"wasm_gzip_bytes":%s,"wasm_brotli_bytes":%s,"js_bytes":%s}\n' \
  "$RHWP_COMMIT" "$hash" "$(wc -l < /out/notices/index.tsv | tr -d ' ')" "$(size "$out/rhwp_bg.wasm")" \
  "$(size "$out/rhwp_bg.wasm.gz")" "$(size "$out/rhwp_bg.wasm.br")" "$(size "$out/rhwp.js")" > /out/report.json
cat /out/report.json
