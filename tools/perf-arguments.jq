# jq -e -f tools/perf-arguments.jq screens.json
# Replay the first observed request in repeat 1 of home, my-tasks and calendar.
# Keep each screen's timezone and scope, plus request IDs as evidence.
def request($screen; $pattern):
  [.[] | select(.screen == $screen and .repeat == 1) | .requests[] |
    select(.method == "GET" and (.path | test($pattern)))] |
  min_by(.start) // error("missing replay request: " + $screen);
def required($query; $name):
  $query[$name] |
  if type == "string" and length > 0 then . else error("missing replay argument: " + $name) end;
. as $screens |
request("home"; "^/api/me/home$") as $home |
$screens | request("my-tasks"; "^/api/me/tasks$") as $tasks |
$screens | request("calendar"; "^/api/workspaces/[^/]+/calendar$") as $calendar |
{
  home_org: required($home.query; "organization_id"),
  home_tz: required($home.query; "tz"),
  tasks_org: required($tasks.query; "organization_id"),
  today: required($tasks.query; "today"),
  calendar_workspace: ($calendar.path | capture("^/api/workspaces/(?<id>[^/]+)/calendar$").id),
  calendar_from: required($calendar.query; "from"),
  calendar_to: required($calendar.query; "to"),
  calendar_tz: required($calendar.query; "tz"),
  calendar_module: ($calendar.query.module_id // ""),
  calendar_item: ($calendar.query.item_id // ""),
  evidence: {home: $home.requestId, tasks: $tasks.requestId, calendar: $calendar.requestId}
}
