# jq -f tools/perf-depth.jq screens.json
# API dependency witnesses for #211's parallel source boundaries (not a timing heuristic).
# Edges follow App.load, HomeDashboard, ItemScreen, TaskBoard.loadItem,
# detail-data.loadDetail and DocumentScreen. Each observed edge must finish before its child.
# Every parallel arm is checked. Session/home fan-outs contribute one level;
# ItemScreen joins documents with item -> module before rendering the selected view.
# Task detail joins its task resource with six item-scope reads sharing an overlap.
def pick($requests; $pattern; $after; $last):
  [$requests[] | select(.path | test($pattern)) | select(.start >= $after)] |
  if length == 0 then error("missing dependency witness: " + $pattern)
  elif $last then max_by(.start) else min_by(.start) end;
def branch($requests; $pattern; $after):
  [$requests[] | select(.path | test($pattern))] as $matches |
  if ($matches | length) == 0 then error("missing dependency witness: " + $pattern)
  elif any($matches[]; .start < $after) then error("early dependency witness: " + $pattern)
  else $matches end;
def require_overlap($requests):
  ($requests | to_entries) as $arms |
  reduce $arms[] as $arm (.;
    # Exclude the request itself: its own end cannot witness parallelism.
    # Both interval boundaries must overlap at least one actual sibling.
    if any($arms[] | select(.key != $arm.key);
      $arm.value.start < .value.end and .value.start < $arm.value.end)
    then .
    else error("non-overlapping dependency witness: " + $arm.value.path) end);
def item_screen($screen):
  ["board", "gantt", "task-detail", "document-tree"] | index($screen) != null;
# #291 explicit-task reads. Older traces retain their original witnesses below.
def once($requests; $pattern; $after):
  branch($requests; $pattern; $after) |
  if length != 1 then error("duplicate dependency witness: " + $pattern) else .[0] end;
def task_screen_chain($sample; $requests; $root; $arms):
  ([$requests[] | select(.path | startswith("/api/"))]) as $api |
  if any($api[]; (.start | type) != "number" or (.end | type) != "number" or .start < 0 or .end < .start)
  then error("unfinished dependency witness")
  elif ($api | length) != ($api | unique_by([.path,.start,.end,.requestId]) | length)
  then error("duplicate dependency witness record") else . end |
  once($requests; "^/api/items/[^/]+/task-context$"; $root.end) as $context |
  require_overlap($arms + [$context]) |
  if ($arms + [$context] | map(.start) | max) >= ($arms + [$context] | map(.end) | min)
  then error("non-overlapping dependency witness: task context gate arms") else . end |
  ($arms + [$context] | max_by(.end)) as $join |
  once($requests; "^/api/items/[^/]+/recurrences/sync$"; $join.end) as $sync |
  once($requests; "^/api/items/[^/]+/task-board$"; $sync.end) as $board |
  if $sync.path != ($context.path | sub("/task-context$"; "/recurrences/sync"))
     or $board.path != ($context.path | sub("/task-context$"; "/task-board"))
  then error("wrong task scope dependency witness") else . end |
  if $sample.screen == "task-detail" then
    once($requests; "^/api/tasks/[^/]+/detail-read$"; $join.end) as $detail |
    require_overlap([$sync, $detail]) |
    ($detail.path | capture("^/api/tasks/(?<id>[^/]+)/detail-read$").id) as $id |
    (["^/api/tasks/" + $id + "/comments$", "^/api/tasks/" + $id + "/time$",
      "^/api/tasks/" + $id + "/backlinks$", "^/api/me/marks/task/" + $id + "$"] |
      map(once($requests; .; $detail.end))) as $panels |
    require_overlap($panels) |
    {chain: [$root, $join, $detail, ($panels | max_by(.end))],
     parallel: ($arms + [$context, $sync, $board, $detail] + $panels)}
  else {chain: [$root, $join, $sync, $board], parallel: ($arms + [$context])} end;
def patterns($screen):
  if $screen == "board" or $screen == "gantt" then
    ["^/api/items/[^/]+/recurrences/sync$", "^/api/items/[^/]+/tasks$"]
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
  (if (["board", "gantt", "task-detail"] | index($sample.screen)) != null
      and any($requests[]; .path | test("/(task-context|task-board|detail-read)$")) then
    task_screen_chain($sample; $requests; $root; $arms)
  elif .screen == "home" or .screen == "activity" then
    pick($requests; "^/api/me/home$"; $gate.end; false) as $home |
    (["^/api/workspaces/[^/]+/calendar$", "^/api/me/notifications$", "^/api/me/activity$"] |
      map(branch($requests; .; $home.end)) | add) as $children |
    require_overlap($children) |
    {chain: [$root, $gate, $home, ($children | max_by(.end))], parallel: $children}
  else
    (if item_screen(.screen) then
      pick($requests; "^/api/items/[^/]+$"; $gate.end; false) as $item |
      branch($requests; "^/api/items/[^/]+/documents$"; $gate.end) as $documents |
      if any($documents[]; .start >= $item.end)
      then error("non-overlapping dependency witness: item documents did not start before item completion")
      else . end |
      require_overlap([$item] + $documents) |
      branch($requests; "^/api/modules/[^/]+$"; $item.end) as $modules |
      ($modules | min_by(.start)) as $module |
      {chain: [$root, $gate, $item, $module],
       after: ([$documents[].end, $module.end] | max), parallel: ($documents + [$module])}
    else {chain: [$root, $gate], after: $gate.end, parallel: []} end) as $initial |
    if $sample.screen == "task-detail" then
      pick($requests; "^/api/tasks/[^/]+$"; $initial.after; false) as $task |
      (["^/api/items/[^/]+/tasks$", "^/api/items/[^/]+$", "^/api/task-assignees$",
        "^/api/task-dependencies$", "^/api/task-series$", "^/api/checklist-items$"] |
        map(pick($requests; .; $task.start; false))) as $scope |
      if any($scope[]; .start >= $task.end or .end <= $task.start)
      then error("non-overlapping dependency witness: task detail scope") else . end |
      if ($scope | map(.start) | max) >= ($scope | map(.end) | min)
      then error("non-overlapping dependency witness: task detail scope arms") else . end |
      ([$task] + $scope | max_by(.end)) as $join |
      pick($requests; "^/api/tasks/[^/]+/comments$"; $join.end; false) as $comments |
      $initial + {chain: ($initial.chain + [$join, $comments]),
        parallel: ($initial.parallel + [$task] + $scope)}
    else
      reduce patterns(.screen)[] as $pattern ($initial;
        pick($requests; $pattern; .after; false) as $child |
        .chain += [$child] | .after = $child.end)
    end
  end) |
  {screen: $sample.screen, repeat: $sample.repeat, depth: (.chain | length),
   chain: (.chain | map({path, start, end})), parallel: (.parallel | map({path, start, end}))}
)
