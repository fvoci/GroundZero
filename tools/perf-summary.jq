# jq -s -f tools/perf-summary.jq screens.json access.json
# access.json is an array of the product's JSON http.request log events.
def percentile($p): sort | .[(([length * $p | ceil, 1] | max) - 1)];
def spread: {median: percentile(0.5), min: min, max: max};
.[0] as $screens | .[1] as $access |
($access | map({key: .request_id, value: .duration_ms}) | from_entries) as $durations |
$screens | map(
  .requests as $requests |
  ($requests | map(select(.path | startswith("/api/")))) as $api |
  ($api | map($durations[.requestId]) | map(select(. != null))) as $times |
  if ($times | length) != ($api | length) then error("missing product duration_ms log") else . end |
  {
    screen, repeat, meaningfulMs,
    http: ($requests | length), api: ($api | length),
    responseBytes: ($requests | map(.responseBytes) | add),
    apiResponseBytes: ($api | map(.responseBytes) | add),
    requestBytes: ($requests | map(.requestBytes) | add),
    socketReceivedBytes, socketSentBytes,
    serverSamples: ($times | length),
    p50: ($times | percentile(0.5)), p95: ($times | percentile(0.95))
  }
) | group_by(.screen) | map({
  screen: .[0].screen, runs: .,
  http: (map(.http) | spread), api: (map(.api) | spread),
  responseBytes: (map(.responseBytes) | spread),
  apiResponseBytes: (map(.apiResponseBytes) | spread),
  socketReceivedBytes: (map(.socketReceivedBytes) | spread),
  socketSentBytes: (map(.socketSentBytes) | spread),
  meaningfulMs: (map(.meaningfulMs) | spread),
  p50: (map(.p50) | spread), p95: (map(.p95) | spread)
})
