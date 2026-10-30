# jq -f tools/perf-depth.jq screens.json
# API dependency witnesses for #211's parallel source boundaries (not a timing heuristic).
# Edges follow App.load, HomeDashboard, ItemScreen, TaskBoard.loadItem,
# detail-data.loadDetail and DocumentScreen. Each observed edge must finish before its child.
# Every parallel arm is checked. Session/home fan-outs contribute one level;
# ItemScreen joins documents with item -> module before rendering the selected view.
def pick($requests; $pattern; $after; $last):
  [$requests[] | select(.path | test($pattern)) | select(.start >= $after)] |
  if length == 0 then error("missing dependency witness: " + $pattern)
  elif $last then max_by(.start) else min_by(.start) end;
def branch($requests; $pattern; $after):
  [$requests[] | select(.path | test($pattern))] as $matches |
  if ($matches | length) == 0 then error("missing dependency witness: " + $pattern)
  elif any($matches[]; .start < $after) then error("early dependency witness: " + $pattern)
  else $matches end;
def item_screen($screen):
  ["board", "gantt", "task-detail", "document-tree"] | index($screen) != null;
def patterns($screen):
  if $screen == "board" or $screen == "gantt" then
    ["^/api/items/[^/]+/recurrences/sync$", "^/api/items/[^/]+/tasks$"]
  elif $screen == "task-detail" then
    ["^/api/tasks/[^/]+$", "^/api/items/[^/]+/tasks$", "^/api/tasks/[^/]+/comments$"]
  elif $screen == "document-tree" then
    ["^/api/items/[^/]+/backlinks$"]
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
  (if .screen == "home" or .screen == "activity" then
    pick($requests; "^/api/me/home$"; $gate.end; false) as $home |
    (["^/api/workspaces/[^/]+/calendar$", "^/api/me/notifications$", "^/api/me/activity$"] |
      map(branch($requests; .; $home.end)) | add) as $children |
    {chain: [$root, $gate, $home, ($children | max_by(.end))], parallel: $children}
  else
    (if item_screen(.screen) then
      pick($requests; "^/api/items/[^/]+$"; $gate.end; false) as $item |
      branch($requests; "^/api/items/[^/]+/documents$"; $gate.end) as $documents |
      branch($requests; "^/api/modules/[^/]+$"; $item.end) as $modules |
      ($modules | min_by(.start)) as $module |
      {chain: [$root, $gate, $item, $module],
       after: ([$documents[].end, $module.end] | max), parallel: ($documents + [$module])}
    else {chain: [$root, $gate], after: $gate.end, parallel: []} end) as $initial |
    reduce patterns(.screen)[] as $pattern ($initial;
      pick($requests; $pattern; .after; $sample.screen == "task-detail" and ($pattern | endswith("/tasks$"))) as $child |
      .chain += [$child] | .after = $child.end)
  end) |
  {screen: $sample.screen, repeat: $sample.repeat, depth: (.chain | length),
   chain: (.chain | map({path, start, end})), parallel: (.parallel | map({path, start, end}))}
)
