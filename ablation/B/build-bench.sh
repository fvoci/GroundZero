#!/usr/bin/env bash
set -euo pipefail
export CARGO_TARGET_DIR=target/ablation-body-size/build
cargo build --locked --release -j2 -p serde_json -p uuid
json_rlibs=("$CARGO_TARGET_DIR"/release/deps/libserde_json-*.rlib)
uuid_rlibs=("$CARGO_TARGET_DIR"/release/deps/libuuid-*.rlib)
test "${#json_rlibs[@]}" -eq 1
test "${#uuid_rlibs[@]}" -eq 1
rustc --edition=2024 -O -C debuginfo=1 \
  -L "dependency=$CARGO_TARGET_DIR/release/deps" \
  --extern "serde_json=${json_rlibs[0]}" --extern "uuid=${uuid_rlibs[0]}" \
  target/ablation-body-size/bench.rs -o target/ablation-body-size/bench
