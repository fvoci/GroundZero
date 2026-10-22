#!/usr/bin/env bash
# Same as run_tests with --oidc, but keeps going after a failing binary.
cargo test --workspace --locked --no-fail-fast -- --skip supabase_
