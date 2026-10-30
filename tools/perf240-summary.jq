# jq -s -f tools/perf240-summary.jq sharing.json access.json
.[1] as $access
| .[0]
| map(. as $run | {
    n, repeat, filters,
    http: [.http[] | . as $h | {
      kind, elapsed_ms,
      duration_ms: ([$access[] | select(.request_id == $h.request_id) | .duration_ms] | if length == 1 then .[0] else error("request log missing or duplicated") end),
      result_count: (if (.response | type) == "array" then (.response | length) else null end)
    }],
    db: (.plans | with_entries(.value = {
      execution_ms: .value[0]["Execution Time"],
      hit: .value[0].Plan["Shared Hit Blocks"],
      read: .value[0].Plan["Shared Read Blocks"]
    }))
  })
