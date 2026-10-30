# jq -f tools/perf-depth.jq screens.json
# API dependency witnesses for product baseline 4ab54392 (not a generic timing heuristic).
# Edges follow App.load, HomeDashboard, ItemScreen, TaskBoard.loadItem,
# detail-data.loadDetail and DocumentScreen. Each observed edge must finish before its child.
# The parallel workspaces/organizations gate contributes one level, taking its slower arm.
def pick($requests; $pattern; $after; $last):
  [$requests[] | select(.path | test($pattern)) | select(.start >= $after)] |
  if length == 0 then error("missing dependency witness: " + $pattern)
  elif $last then max_by(.start) else min_by(.start) end;
def patterns($screen):
  if $screen == "home" or $screen == "activity" then
    ["^/api/me/home$", "^/api/workspaces/[^/]+/calendar$", "^/api/me/notifications$", "^/api/me/activity$"]
  elif $screen == "board" or $screen == "gantt" then
    ["^/api/items/[^/]+$", "^/api/items/[^/]+/documents$", "^/api/items/[^/]+/recurrences/sync$", "^/api/items/[^/]+/tasks$"]
  elif $screen == "task-detail" then
    ["^/api/items/[^/]+$", "^/api/items/[^/]+/documents$", "^/api/tasks/[^/]+$", "^/api/items/[^/]+/tasks$", "^/api/tasks/[^/]+/comments$"]
  elif $screen == "document-tree" then
    ["^/api/items/[^/]+$", "^/api/items/[^/]+/documents$", "^/api/items/[^/]+/backlinks$"]
  elif $screen == "document-open" then
    ["^/api/documents/[^/]+$", "^/api/items/[^/]+/documents$"]
  elif $screen == "calendar" then ["^/api/workspaces/[^/]+/calendar$"]
  elif $screen == "my-tasks" then ["^/api/me/tasks$"]
  else error("unknown screen") end;
map(
  . as $sample | .requests as $requests |
  pick($requests; "^/api/me$"; 0; false) as $root |
  ([$requests[] | select(.path == "/api/workspaces" or .path == "/api/me/organizations")]) as $arms |
  if ([$arms[].path] | index("/api/workspaces")) == null
     or ([$arms[].path] | index("/api/me/organizations")) == null
  then error("missing session gate request")
  elif ($arms | map(.start) | min) < $root.end
  then error("session gate order changed") else . end |
  ($arms | max_by(.end)) as $gate |
  reduce patterns(.screen)[] as $pattern ([$root, $gate];
    .[-1].end as $after |
    . + [pick($requests; $pattern; $after; $sample.screen == "task-detail" and ($pattern | endswith("/tasks$")))]) |
  {screen: $sample.screen, repeat: $sample.repeat, depth: length,
   chain: map({path, start, end})}
)
