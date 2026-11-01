#!/usr/bin/env bash
# #291: a full heavy command remains busy between compiler/Node subprocesses.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
check_state() {
  local name=$1 process=$2 compiler=$3 node=$4 heavy=$5
  printf '%s\n' "$process" | awk -f tools/perf-build-state.awk |
    jq -e --argjson compiler "$compiler" --argjson node "$node" --argjson heavy "$heavy" \
      '. == {compiler:$compiler,nodeBuild:$node,heavyCommand:$heavy}' > /dev/null || {
        printf 'FAIL build state: %s\n' "$name" >&2
        return 1
      }
}
check_state quiet 'bash bash target/read-report.sh' false false false
check_state rust 'rustc rustc --crate-name fvoci_server' true false false
check_state clippy 'clippy-driver clippy-driver --crate-name fvoci_server' true false false
check_state linker 'rust-lld rust-lld -flavor gnu' true false false
check_state check-between-stages 'cargo cargo xtask check' false false true
check_state browser-between-stages 'xtask target/debug/xtask browser -- board.spec.ts' false false true
check_state unit-between-stages 'cargo cargo test --workspace --lib --bins --locked' false false true
check_state release-between-stages 'cargo cargo build --release --locked -p fvoci-server' false false true
check_state image 'docker docker buildx build --load .' false false true
check_state compose-image 'docker docker compose build web' false false true
check_state vite 'MainThread node node_modules/vite/bin/vite.js build' false true false
check_state eslint 'node node node_modules/eslint/bin/eslint.js .' false true false
check_state pnpm-between-stages 'MainThread node pnpm --filter @fvoci/web run build' false true false
check_state measurement-browser 'MainThread node pnpm --filter @fvoci/web exec playwright test perf-baseline.spec.ts' false false false
check_state measurement-server 'cargo cargo test --release --locked -p fvoci-server --test screen_measure serve_t6 -- --ignored --exact --nocapture' false false false
check_state measurement-rooms 'cargo cargo test --release --locked -p fvoci-server --test it room_memory_measure::room_memory_measure -- --ignored --exact --nocapture' false false false
check_state other-test-not-server 'cargo cargo test --test it serve_t6' false false true
check_state format 'cargo cargo fmt --all --check' false false false
printf 'PASS build state: 18 command lifecycles and measurement exclusions\n'
