# ps -eo comm,args. Track full commands as well as their compiler/Node children.
$1 ~ /^(rustc|clippy-driver|cc1|cc1plus|ld|rust-lld)$/ { compiler=1 }
$1 ~ /^(node|MainThread)$/ &&
    (/vite|vitest|storybook|svelte-check|eslint|tsc/ ||
     /(pnpm|npm|bun).* run (build|check|test|lint|build-storybook)([[:space:]]|$)/) { node=1 }
($1 == "cargo" || $1 ~ /^xtask(-|$)/) {
    if (/(^|[\/[:space:]])xtask[^[:space:]]*[[:space:]]+(check|browser)([[:space:]]|$)/ ||
        ($1 == "cargo" && /(^|[[:space:]])(build|test)([[:space:]]|$)/)) {
        # These are the measurement itself, prebuilt before the sampling marker.
        if (!/--test[[:space:]]+screen_measure[[:space:]]+serve_t6([[:space:]]|$)/ &&
            !/--test[[:space:]]+it[[:space:]]+room_memory_measure::room_memory_measure([[:space:]]|$)/) heavy=1
    }
}
$1 ~ /^(docker|podman)$/ && /(^|[[:space:]])(--build|build)([[:space:]]|$)/ { heavy=1 }
END {
    printf "{\"compiler\":%s,\"nodeBuild\":%s,\"heavyCommand\":%s}\n", \
        compiler ? "true" : "false", node ? "true" : "false", heavy ? "true" : "false"
}
