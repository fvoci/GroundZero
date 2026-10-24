#!/usr/bin/env bash
set -euo pipefail
out=target/ablation-body-size
affinity=()
if command -v taskset >/dev/null; then
    cpu_list=$(sed -n 's/^Cpus_allowed_list:[[:space:]]*//p' /proc/self/status)
    cpu_first=${cpu_list%%[-,]*}
    affinity=(taskset -c "$cpu_first")
fi
{
    date -u +%FT%TZ
    uname -a
    printf 'affinity command:'
    printf ' %s' "${affinity[@]}"
    printf '\n'
    cat /proc/loadavg
    sha256sum "$out/bench" Cargo.lock rust-toolchain.toml
} > "$out/measurement-environment.txt"
for run in 1 2 3; do
    "${affinity[@]}" "$out/bench" > "$out/warm-$run.csv"
done
printf 'kind,fixture,pair,variant,iterations,total_ns,serialized_bytes,vec_capacity_bytes\n' > "$out/first-validator.csv"
for fixture in minimal small_prose unicode_escapes many_nodes large_exact large_rejected deep references; do
    for pair in $(seq 1 12); do
        order=(baseline variant)
        if (( pair % 2 == 0 )); then order=(variant baseline); fi
        for variant in "${order[@]}"; do
            "${affinity[@]}" "$out/bench" --first-validator "$variant" "$fixture" >> "$out/first-validator.csv"
        done
    done
done
date -u +%FT%TZ >> "$out/measurement-environment.txt"
cat /proc/loadavg >> "$out/measurement-environment.txt"
